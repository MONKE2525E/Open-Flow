#import <ApplicationServices/ApplicationServices.h>
#import <CoreFoundation/CoreFoundation.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

// Forward declarations for private/semi-private AXTextMarker APIs to prevent pointer truncation or compile warnings on 64-bit systems.
extern CFTypeID AXTextMarkerGetTypeID(void);
extern CFTypeID AXTextMarkerRangeGetTypeID(void);
extern CFIndex AXTextMarkerGetLength(AXTextMarkerRef marker);
extern const UInt8 *AXTextMarkerGetBytePtr(AXTextMarkerRef marker);
extern AXTextMarkerRangeRef AXTextMarkerRangeCreate(CFAllocatorRef allocator, AXTextMarkerRef start_marker, AXTextMarkerRef end_marker);
extern AXTextMarkerRef AXTextMarkerRangeCopyStartMarker(AXTextMarkerRangeRef range);
extern AXTextMarkerRef AXTextMarkerRangeCopyEndMarker(AXTextMarkerRangeRef range);


#define OF_SOURCE_CARET_LOCAL 0
#define OF_SOURCE_EMPTY_FIELD 1
#define OF_SOURCE_AMBIGUOUS_SELECTION 2
#define OF_SOURCE_PERMISSION_MISSING 3
#define OF_SOURCE_UNSUPPORTED_CONTROL 4
#define OF_SOURCE_UNAVAILABLE 5

#define OF_SELECTION_COLLAPSED 0
#define OF_SELECTION_NON_COLLAPSED 1
#define OF_SELECTION_UNKNOWN 2

static const CFStringRef kOFAXHighestEditableAncestorAttribute = CFSTR("AXHighestEditableAncestor");
static const CFStringRef kOFAXSelectedTextMarkerRangeAttribute = CFSTR("AXSelectedTextMarkerRange");
static const CFStringRef kOFAXIndexForTextMarkerParameterizedAttribute = CFSTR("AXIndexForTextMarker");
static const CFStringRef kOFAXStringForTextMarkerRangeParameterizedAttribute = CFSTR("AXStringForTextMarkerRange");
static const CFStringRef kOFAXTextMarkerForIndexParameterizedAttribute = CFSTR("AXTextMarkerForIndex");
static const CFStringRef kOFAXWebAreaRole = CFSTR("AXWebArea");

typedef struct VerenuMacosContextProbeResult {
    int source;
    int selection_state;
    int pid;
    int left_reliable;
    int right_reliable;
    char control_type[64];
    char role[64];
    char subrole[64];
    char identifier[128];
    char title[160];
    char tail[512];
    char head[512];
} VerenuMacosContextProbeResult;

static void of_zero_result(VerenuMacosContextProbeResult *out_result) {
    memset(out_result, 0, sizeof(*out_result));
    out_result->source = OF_SOURCE_UNAVAILABLE;
    out_result->selection_state = OF_SELECTION_UNKNOWN;
}

static void of_write_c_string(const char *value, char *dest, size_t capacity) {
    if (capacity == 0) {
        return;
    }
    if (value == NULL) {
        dest[0] = '\0';
        return;
    }
    snprintf(dest, capacity, "%s", value);
}

static void of_write_cf_string(CFStringRef value, char *dest, size_t capacity) {
    if (capacity == 0) {
        return;
    }
    dest[0] = '\0';
    if (value == NULL) {
        return;
    }
    if (!CFStringGetCString(value, dest, (CFIndex)capacity, kCFStringEncodingUTF8)) {
        dest[0] = '\0';
    }
}

static void of_set_timeout(AXUIElementRef element) {
    if (element != NULL) {
        AXUIElementSetMessagingTimeout(element, 0.015f);
    }
}

static CFTypeRef of_copy_attribute(AXUIElementRef element, CFStringRef attribute) {
    if (element == NULL || attribute == NULL) {
        return NULL;
    }

    CFTypeRef value = NULL;
    AXError err = AXUIElementCopyAttributeValue(element, attribute, &value);
    if (err != kAXErrorSuccess) {
        return NULL;
    }
    return value;
}

static CFTypeRef of_copy_parameterized_attribute(
    AXUIElementRef element,
    CFStringRef attribute,
    CFTypeRef parameter
) {
    if (element == NULL || attribute == NULL || parameter == NULL) {
        return NULL;
    }

    CFTypeRef value = NULL;
    AXError err = AXUIElementCopyParameterizedAttributeValue(element, attribute, parameter, &value);
    if (err != kAXErrorSuccess) {
        return NULL;
    }
    return value;
}

static bool of_copy_string_attribute(
    AXUIElementRef element,
    CFStringRef attribute,
    char *dest,
    size_t capacity
) {
    CFTypeRef value = of_copy_attribute(element, attribute);
    if (value == NULL) {
        return false;
    }

    bool ok = false;
    if (CFGetTypeID(value) == CFStringGetTypeID()) {
        of_write_cf_string((CFStringRef)value, dest, capacity);
        ok = true;
    }
    CFRelease(value);
    return ok;
}

static bool of_copy_bool_attribute(AXUIElementRef element, CFStringRef attribute, bool *out_value) {
    if (out_value == NULL) {
        return false;
    }

    CFTypeRef value = of_copy_attribute(element, attribute);
    if (value == NULL) {
        return false;
    }

    bool ok = false;
    if (CFGetTypeID(value) == CFBooleanGetTypeID()) {
        *out_value = CFBooleanGetValue((CFBooleanRef)value);
        ok = true;
    }
    CFRelease(value);
    return ok;
}

static bool of_copy_range_attribute(AXUIElementRef element, CFStringRef attribute, CFRange *out_range) {
    if (out_range == NULL) {
        return false;
    }

    CFTypeRef value = of_copy_attribute(element, attribute);
    if (value == NULL) {
        return false;
    }

    bool ok = false;
    if (CFGetTypeID(value) == AXValueGetTypeID() &&
        AXValueGetType((AXValueRef)value) == kAXValueTypeCFRange) {
        ok = AXValueGetValue((AXValueRef)value, kAXValueTypeCFRange, out_range);
    }
    CFRelease(value);
    return ok;
}

static bool of_copy_ax_element_attribute(AXUIElementRef element, CFStringRef attribute, AXUIElementRef *out_element) {
    if (out_element == NULL) {
        return false;
    }

    CFTypeRef value = of_copy_attribute(element, attribute);
    if (value == NULL) {
        return false;
    }

    bool ok = false;
    if (CFGetTypeID(value) == AXUIElementGetTypeID()) {
        *out_element = (AXUIElementRef)value;
        ok = true;
    } else {
        CFRelease(value);
    }
    return ok;
}

static bool of_cf_string_equals(CFStringRef a, CFStringRef b) {
    return a != NULL && b != NULL && CFStringCompare(a, b, 0) == kCFCompareEqualTo;
}

static bool of_marker_bytes_equal(AXTextMarkerRef a, AXTextMarkerRef b) {
    if (a == NULL || b == NULL) {
        return false;
    }
    CFIndex a_len = AXTextMarkerGetLength(a);
    CFIndex b_len = AXTextMarkerGetLength(b);
    if (a_len != b_len) {
        return false;
    }
    const UInt8 *a_bytes = AXTextMarkerGetBytePtr(a);
    const UInt8 *b_bytes = AXTextMarkerGetBytePtr(b);
    if (a_bytes == NULL || b_bytes == NULL) {
        return false;
    }
    return memcmp(a_bytes, b_bytes, (size_t)a_len) == 0;
}

static void of_set_control_type(
    VerenuMacosContextProbeResult *out_result,
    const char *mode
) {
    if (out_result->role[0] != '\0') {
        snprintf(out_result->control_type, sizeof(out_result->control_type), "%s:%s", mode, out_result->role);
    } else {
        of_write_c_string(mode, out_result->control_type, sizeof(out_result->control_type));
    }
}

static bool of_copy_value_slices(
    AXUIElementRef element,
    CFIndex selection_start,
    CFIndex selection_end,
    int context_chars,
    char *tail_dest,
    size_t tail_capacity,
    char *head_dest,
    size_t head_capacity,
    bool *out_field_empty
) {
    if (selection_start < 0 || selection_end < selection_start) {
        return false;
    }

    CFTypeRef value = of_copy_attribute(element, kAXValueAttribute);
    if (value == NULL) {
        return false;
    }

    bool ok = false;
    if (CFGetTypeID(value) == CFStringGetTypeID()) {
        CFStringRef full_value = (CFStringRef)value;
        CFIndex full_length = CFStringGetLength(full_value);
        if (out_field_empty != NULL) {
            *out_field_empty = full_length == 0;
        }
        if (full_length == 0) {
            ok = true;
            tail_dest[0] = '\0';
            head_dest[0] = '\0';
        } else if (selection_end <= full_length) {
            CFIndex start = selection_start - context_chars;
            if (start < 0) {
                start = 0;
            }
            CFIndex right_end = selection_end + context_chars;
            if (right_end > full_length) {
                right_end = full_length;
            }
            CFStringRef tail = CFStringCreateWithSubstring(
                kCFAllocatorDefault,
                full_value,
                CFRangeMake(start, selection_start - start)
            );
            CFStringRef head = CFStringCreateWithSubstring(
                kCFAllocatorDefault,
                full_value,
                CFRangeMake(selection_end, right_end - selection_end)
            );
            if (tail != NULL) {
                of_write_cf_string(tail, tail_dest, tail_capacity);
                CFRelease(tail);
            }
            if (head != NULL) {
                of_write_cf_string(head, head_dest, head_capacity);
                CFRelease(head);
            }
            ok = tail != NULL && head != NULL;
        }
    }

    CFRelease(value);
    return ok;
}

static bool of_try_public_text_range(
    AXUIElementRef element,
    int lookbehind_chars,
    VerenuMacosContextProbeResult *out_result
) {
    CFRange range;
    if (!of_copy_range_attribute(element, kAXSelectedTextRangeAttribute, &range)) {
        return false;
    }

    out_result->selection_state = (range.length > 0) ? OF_SELECTION_NON_COLLAPSED : OF_SELECTION_COLLAPSED;
    of_set_control_type(out_result, "cfrange");

    bool field_empty = false;
    if (of_copy_value_slices(
            element,
            range.location,
            range.location + range.length,
            lookbehind_chars,
            out_result->tail,
            sizeof(out_result->tail),
            out_result->head,
            sizeof(out_result->head),
            &field_empty)) {
        out_result->left_reliable = 1;
        out_result->right_reliable = 1;
        out_result->source = field_empty ? OF_SOURCE_EMPTY_FIELD : OF_SOURCE_CARET_LOCAL;
        return true;
    }

    if (range.length > 0) {
        out_result->source = OF_SOURCE_AMBIGUOUS_SELECTION;
        return true;
    }

    CFIndex start = range.location - lookbehind_chars;
    if (start < 0) {
        start = 0;
    }
    CFRange lookbehind = CFRangeMake(start, range.location - start);
    AXValueRef lookbehind_value = AXValueCreate(kAXValueTypeCFRange, &lookbehind);
    if (lookbehind_value != NULL) {
        CFTypeRef substring = of_copy_parameterized_attribute(
            element,
            kAXStringForRangeParameterizedAttribute,
            lookbehind_value
        );
        CFRelease(lookbehind_value);

        if (substring != NULL && CFGetTypeID(substring) == CFStringGetTypeID()) {
            of_write_cf_string((CFStringRef)substring, out_result->tail, sizeof(out_result->tail));
            CFRelease(substring);
            if (out_result->tail[0] != '\0') {
                out_result->left_reliable = 1;
                out_result->source = OF_SOURCE_CARET_LOCAL;
                return true;
            }
        } else if (substring != NULL) {
            CFRelease(substring);
        }
    }

    return false;
}

static bool of_copy_marker_index(
    AXUIElementRef element,
    AXTextMarkerRef marker,
    int64_t *out_index
) {
    if (marker == NULL || out_index == NULL) {
        return false;
    }
    CFTypeRef value = of_copy_parameterized_attribute(
        element,
        kOFAXIndexForTextMarkerParameterizedAttribute,
        marker
    );
    if (value == NULL || CFGetTypeID(value) != CFNumberGetTypeID()) {
        if (value != NULL) {
            CFRelease(value);
        }
        return false;
    }
    bool ok = CFNumberGetValue((CFNumberRef)value, kCFNumberSInt64Type, out_index);
    CFRelease(value);
    return ok;
}

static AXTextMarkerRef of_copy_marker_for_index(
    AXUIElementRef element,
    int64_t index
) {
    CFNumberRef number = CFNumberCreate(kCFAllocatorDefault, kCFNumberSInt64Type, &index);
    if (number == NULL) {
        return NULL;
    }
    CFTypeRef value = of_copy_parameterized_attribute(
        element,
        kOFAXTextMarkerForIndexParameterizedAttribute,
        number
    );
    CFRelease(number);
    if (value == NULL || CFGetTypeID(value) != AXTextMarkerGetTypeID()) {
        if (value != NULL) {
            CFRelease(value);
        }
        return NULL;
    }
    return (AXTextMarkerRef)value;
}

static bool of_copy_marker_string(
    AXUIElementRef element,
    AXTextMarkerRef start,
    AXTextMarkerRef end,
    char *dest,
    size_t capacity
) {
    AXTextMarkerRangeRef range = AXTextMarkerRangeCreate(kCFAllocatorDefault, start, end);
    if (range == NULL) {
        return false;
    }
    CFTypeRef value = of_copy_parameterized_attribute(
        element,
        kOFAXStringForTextMarkerRangeParameterizedAttribute,
        range
    );
    CFRelease(range);
    if (value == NULL || CFGetTypeID(value) != CFStringGetTypeID()) {
        if (value != NULL) {
            CFRelease(value);
        }
        return false;
    }
    of_write_cf_string((CFStringRef)value, dest, capacity);
    CFRelease(value);
    return true;
}

static bool of_try_text_marker_range(
    AXUIElementRef element,
    int lookbehind_chars,
    VerenuMacosContextProbeResult *out_result
) {
    CFTypeRef selected_range_value = of_copy_attribute(element, kOFAXSelectedTextMarkerRangeAttribute);
    if (selected_range_value == NULL) {
        return false;
    }
    if (CFGetTypeID(selected_range_value) != AXTextMarkerRangeGetTypeID()) {
        CFRelease(selected_range_value);
        return false;
    }

    AXTextMarkerRangeRef selected_range = (AXTextMarkerRangeRef)selected_range_value;
    AXTextMarkerRef start_marker = AXTextMarkerRangeCopyStartMarker(selected_range);
    AXTextMarkerRef end_marker = AXTextMarkerRangeCopyEndMarker(selected_range);
    bool collapsed = of_marker_bytes_equal(start_marker, end_marker);

    out_result->selection_state = collapsed ? OF_SELECTION_COLLAPSED : OF_SELECTION_NON_COLLAPSED;
    of_set_control_type(out_result, "text_marker");

    int64_t start_index = 0;
    int64_t end_index = 0;
    bool indices_ok = of_copy_marker_index(element, start_marker, &start_index)
        && of_copy_marker_index(element, end_marker, &end_index);
    if (!indices_ok || start_index < 0 || end_index < start_index) {
        if (start_marker != NULL) CFRelease(start_marker);
        if (end_marker != NULL) CFRelease(end_marker);
        CFRelease(selected_range);
        return false;
    }

    bool field_empty = false;
    if (of_copy_value_slices(
            element,
            (CFIndex)start_index,
            (CFIndex)end_index,
            lookbehind_chars,
            out_result->tail,
            sizeof(out_result->tail),
            out_result->head,
            sizeof(out_result->head),
            &field_empty)) {
        out_result->left_reliable = 1;
        out_result->right_reliable = 1;
        out_result->source = field_empty ? OF_SOURCE_EMPTY_FIELD : OF_SOURCE_CARET_LOCAL;
        if (start_marker != NULL) CFRelease(start_marker);
        if (end_marker != NULL) CFRelease(end_marker);
        CFRelease(selected_range);
        return true;
    }

    int64_t lookbehind_index = start_index - lookbehind_chars;
    if (lookbehind_index < 0) {
        lookbehind_index = 0;
    }
    AXTextMarkerRef lookbehind_marker = start_index > 0
        ? of_copy_marker_for_index(element, lookbehind_index)
        : NULL;
    bool tail_ok = start_index == 0;
    if (lookbehind_marker != NULL) {
        tail_ok = of_copy_marker_string(
            element,
            lookbehind_marker,
            start_marker,
            out_result->tail,
            sizeof(out_result->tail)
        ) && out_result->tail[0] != '\0';
        CFRelease(lookbehind_marker);
    }
    out_result->left_reliable = tail_ok ? 1 : 0;

    // One character is enough on the right: spacing depends on the immediate
    // insertion edge, including whether it is whitespace or closing punctuation.
    AXTextMarkerRef lookahead_marker = of_copy_marker_for_index(element, end_index + 1);
    bool head_ok = true;
    if (lookahead_marker != NULL) {
        head_ok = of_copy_marker_string(
            element,
            end_marker,
            lookahead_marker,
            out_result->head,
            sizeof(out_result->head)
        ) && out_result->head[0] != '\0';
        CFRelease(lookahead_marker);
        out_result->right_reliable = head_ok ? 1 : 0;
    } else {
        out_result->head[0] = '\0';
        out_result->right_reliable = 0;
    }

    // Index zero proves there is nothing before the caret only when the probe
    // also proves the document continues on the right. With no AXValue and no
    // readable lookahead, index zero may be a provider fallback rather than an
    // empty field, so preserving the payload is safer than capitalizing it.
    if (start_index == 0 && end_index == 0 && !out_result->right_reliable) {
        out_result->left_reliable = 0;
    }

    if (out_result->left_reliable || out_result->right_reliable) {
        out_result->source = OF_SOURCE_CARET_LOCAL;
    } else {
        out_result->source = collapsed ? OF_SOURCE_UNAVAILABLE : OF_SOURCE_AMBIGUOUS_SELECTION;
    }

    if (start_marker != NULL) CFRelease(start_marker);
    if (end_marker != NULL) CFRelease(end_marker);
    CFRelease(selected_range);
    return true;
}

int verenu_macos_read_context_probe(
    int lookbehind_chars,
    VerenuMacosContextProbeResult *out_result
) {
    if (out_result == NULL) {
        return 0;
    }

    of_zero_result(out_result);

    if (!AXIsProcessTrusted()) {
        out_result->source = OF_SOURCE_PERMISSION_MISSING;
        of_write_c_string("permission_missing", out_result->control_type, sizeof(out_result->control_type));
        return 1;
    }

    AXUIElementRef system = AXUIElementCreateSystemWide();
    if (system == NULL) {
        of_write_c_string("systemwide_unavailable", out_result->control_type, sizeof(out_result->control_type));
        return 1;
    }
    of_set_timeout(system);

    AXUIElementRef focused_app = NULL;
    if (of_copy_ax_element_attribute(system, kAXFocusedApplicationAttribute, &focused_app)) {
        of_set_timeout(focused_app);
    }

    AXUIElementRef focused_element = NULL;
    if (!of_copy_ax_element_attribute(system, kAXFocusedUIElementAttribute, &focused_element)) {
        if (focused_app != NULL) {
            CFRelease(focused_app);
        }
        CFRelease(system);
        of_write_c_string("focused_element_unavailable", out_result->control_type, sizeof(out_result->control_type));
        return 1;
    }
    of_set_timeout(focused_element);

    AXUIElementRef target_element = focused_element;
    AXUIElementRef editable_ancestor = NULL;
    if (of_copy_ax_element_attribute(focused_element, kOFAXHighestEditableAncestorAttribute, &editable_ancestor)) {
        if (editable_ancestor != NULL) {
            target_element = editable_ancestor;
            of_set_timeout(target_element);
        }
    }

    pid_t pid = 0;
    AXUIElementGetPid(target_element, &pid);
    out_result->pid = (int)pid;

    bool has_role = of_copy_string_attribute(target_element, kAXRoleAttribute, out_result->role, sizeof(out_result->role));
    of_copy_string_attribute(target_element, kAXSubroleAttribute, out_result->subrole, sizeof(out_result->subrole));
    of_copy_string_attribute(target_element, kAXIdentifierAttribute, out_result->identifier, sizeof(out_result->identifier));
    of_copy_string_attribute(target_element, kAXTitleAttribute, out_result->title, sizeof(out_result->title));

    bool is_secure = out_result->subrole[0] != '\0' &&
        strcmp(out_result->subrole, "AXSecureTextField") == 0;
    if (is_secure) {
        out_result->source = OF_SOURCE_UNSUPPORTED_CONTROL;
        of_write_c_string("secure_text_field", out_result->control_type, sizeof(out_result->control_type));
        if (editable_ancestor != NULL) {
            CFRelease(editable_ancestor);
        }
        CFRelease(focused_element);
        if (focused_app != NULL) {
            CFRelease(focused_app);
        }
        CFRelease(system);
        return 1;
    }

    bool is_editable = false;
    bool editable_known = of_copy_bool_attribute(target_element, kAXIsEditableAttribute, &is_editable);
    bool is_web_area = false;
    if (has_role) {
        CFStringRef role_value = CFStringCreateWithCString(kCFAllocatorDefault, out_result->role, kCFStringEncodingUTF8);
        if (role_value != NULL) {
            is_web_area = of_cf_string_equals(role_value, kOFAXWebAreaRole);
            CFRelease(role_value);
        }
    }

    bool handled = of_try_public_text_range(target_element, lookbehind_chars, out_result);
    if (!handled) {
        handled = of_try_text_marker_range(target_element, lookbehind_chars, out_result);
    }

    if (!handled) {
        if ((editable_known && !is_editable) || !has_role) {
            out_result->source = OF_SOURCE_UNSUPPORTED_CONTROL;
            of_write_c_string("unsupported_control", out_result->control_type, sizeof(out_result->control_type));
        } else if (is_web_area) {
            out_result->source = OF_SOURCE_UNAVAILABLE;
            of_write_c_string("text_marker_unavailable", out_result->control_type, sizeof(out_result->control_type));
        } else {
            out_result->source = OF_SOURCE_UNAVAILABLE;
            of_write_c_string("context_unavailable", out_result->control_type, sizeof(out_result->control_type));
        }
    }

    if (editable_ancestor != NULL) {
        CFRelease(editable_ancestor);
    }
    CFRelease(focused_element);
    if (focused_app != NULL) {
        CFRelease(focused_app);
    }
    CFRelease(system);
    return 1;
}
