#include "extended_titlebar.h"

#include <windows.h>
#include <roapi.h>
#include <mutex>

#include <WindowsAppSDK-VersionInfo.h>
#include <MddBootstrap.h>

#include <winrt/Microsoft.UI.Interop.h>
#include <winrt/Microsoft.UI.Windowing.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.UI.h>
#include <winrt/base.h>

STDAPI WindowsAppRuntime_EnsureIsLoaded();

namespace {

constexpr char kWindowsAppSdkBridgeVersion[] = "1.8.260710003";

using winrt::Microsoft::UI::Windowing::AppWindow;
using winrt::Microsoft::UI::Windowing::AppWindowTitleBar;
using winrt::Windows::Foundation::IReference;
using winrt::Windows::UI::Color;

HRESULT ensure_winrt_apartment() noexcept {
    const HRESULT result = RoInitialize(RO_INIT_SINGLETHREADED);
    if (result == RPC_E_CHANGED_MODE) {
        return S_OK;
    }
    return result;
}

HRESULT ensure_windows_app_sdk() noexcept {
    static std::once_flag once;
    static HRESULT result = E_UNEXPECTED;
    std::call_once(once, [] {
        result = ::MddBootstrapInitialize(
            WINDOWSAPPSDK_RELEASE_MAJORMINOR,
            WINDOWSAPPSDK_RELEASE_VERSION_TAG_W,
            PACKAGE_VERSION{WINDOWSAPPSDK_RUNTIME_VERSION_UINT64});
    });
    return result;
}

AppWindow app_window_for(HWND hwnd) {
    const auto window_id = winrt::Microsoft::UI::GetWindowIdFromWindow(hwnd);
    return AppWindow::GetFromWindowId(window_id);
}

Color color(uint8_t alpha, uint8_t red, uint8_t green, uint8_t blue) noexcept {
    return Color{alpha, red, green, blue};
}

IReference<Color> nullable_color(Color const& value) {
    return winrt::box_value(value).as<IReference<Color>>();
}

void apply_theme(AppWindowTitleBar const& titlebar, bool dark) {
    const Color foreground = dark
        ? color(255, 247, 239, 230)
        : color(255, 43, 36, 34);
    const Color hover = dark
        ? color(32, 247, 239, 230)
        : color(24, 43, 36, 34);
    const Color pressed = dark
        ? color(48, 247, 239, 230)
        : color(40, 43, 36, 34);

    titlebar.ButtonForegroundColor(nullable_color(foreground));
    titlebar.ButtonInactiveForegroundColor(nullable_color(color(150, foreground.R, foreground.G, foreground.B)));
    titlebar.ButtonBackgroundColor(nullable_color(color(0, 0, 0, 0)));
    titlebar.ButtonInactiveBackgroundColor(nullable_color(color(0, 0, 0, 0)));
    titlebar.ButtonHoverBackgroundColor(nullable_color(hover));
    titlebar.ButtonPressedBackgroundColor(nullable_color(pressed));
}

HRESULT read_metrics(HWND hwnd, AppWindowTitleBar const& titlebar, VerenuTitleBarMetrics* metrics) noexcept {
    if (!metrics) {
        return E_POINTER;
    }

    RECT window_rect{};
    RECT client_rect{};
    POINT client_origin{};
    if (!GetWindowRect(hwnd, &window_rect) ||
        !GetClientRect(hwnd, &client_rect) ||
        !ClientToScreen(hwnd, &client_origin)) {
        return HRESULT_FROM_WIN32(GetLastError());
    }

    metrics->titlebar_height = titlebar.Height();
    metrics->left_inset = titlebar.LeftInset();
    metrics->right_inset = titlebar.RightInset();
    metrics->dpi = GetDpiForWindow(hwnd);
    metrics->window_left = window_rect.left;
    metrics->window_top = window_rect.top;
    metrics->window_right = window_rect.right;
    metrics->window_bottom = window_rect.bottom;
    metrics->client_left = client_rect.left;
    metrics->client_top = client_rect.top;
    metrics->client_right = client_rect.right;
    metrics->client_bottom = client_rect.bottom;
    metrics->client_screen_x = client_origin.x;
    metrics->client_screen_y = client_origin.y;
    metrics->is_maximized = IsZoomed(hwnd) ? 1 : 0;
    metrics->extends_content = titlebar.ExtendsContentIntoTitleBar() ? 1 : 0;
    return S_OK;
}

struct WebViewSearch {
    HWND caption_controls{};
};

BOOL CALLBACK find_largest_child(HWND child, LPARAM context) {
    auto* search = reinterpret_cast<WebViewSearch*>(context);
    wchar_t class_name[64]{};
    GetClassNameW(child, class_name, 64);
    if (wcscmp(class_name, L"ReunionWindowingCaptionControls") == 0) {
        search->caption_controls = child;
    }
    return TRUE;
}

HRESULT reveal_native_caption_controls(HWND hwnd) noexcept {
    WebViewSearch search{};
    EnumChildWindows(hwnd, find_largest_child, reinterpret_cast<LPARAM>(&search));
    if (search.caption_controls) {
        SetWindowPos(search.caption_controls, HWND_TOP, 0, 0, 0, 0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
    }
    return S_OK;
}

HRESULT configure(HWND hwnd, bool dark, bool enable, VerenuTitleBarMetrics* metrics) {
    (void)kWindowsAppSdkBridgeVersion;
    if (!IsWindow(hwnd)) {
        return E_HANDLE;
    }

    winrt::check_hresult(ensure_windows_app_sdk());
    winrt::check_hresult(WindowsAppRuntime_EnsureIsLoaded());
    winrt::check_hresult(ensure_winrt_apartment());
    const AppWindow app_window = app_window_for(hwnd);
    if (!app_window) {
        return E_FAIL;
    }

    const AppWindowTitleBar titlebar = app_window.TitleBar();
    if (!AppWindowTitleBar::IsCustomizationSupported()) {
        return HRESULT_FROM_WIN32(ERROR_NOT_SUPPORTED);
    }

    if (enable) {
        titlebar.ExtendsContentIntoTitleBar(true);
    }
    apply_theme(titlebar, dark);
    winrt::check_hresult(reveal_native_caption_controls(hwnd));
    return read_metrics(hwnd, titlebar, metrics);
}

template <typename Callback>
int32_t ffi_guard(Callback&& callback) noexcept {
    try {
        return static_cast<int32_t>(callback());
    } catch (winrt::hresult_error const& error) {
        return error.code().value;
    } catch (...) {
        return E_UNEXPECTED;
    }
}

} // namespace

int32_t verenu_enable_extended_titlebar(
    intptr_t hwnd,
    int32_t dark,
    VerenuTitleBarMetrics* metrics) {
    return ffi_guard([&] {
        return configure(reinterpret_cast<HWND>(hwnd), dark != 0, true, metrics);
    });
}

int32_t verenu_update_extended_titlebar(
    intptr_t hwnd,
    int32_t dark,
    VerenuTitleBarMetrics* metrics) {
    return ffi_guard([&] {
        return configure(reinterpret_cast<HWND>(hwnd), dark != 0, false, metrics);
    });
}

int32_t verenu_get_extended_titlebar_metrics(
    intptr_t hwnd,
    VerenuTitleBarMetrics* metrics) {
    return ffi_guard([&] {
        if (!IsWindow(reinterpret_cast<HWND>(hwnd))) {
            return E_HANDLE;
        }
        winrt::check_hresult(ensure_windows_app_sdk());
        winrt::check_hresult(WindowsAppRuntime_EnsureIsLoaded());
        winrt::check_hresult(ensure_winrt_apartment());
        const AppWindow app_window = app_window_for(reinterpret_cast<HWND>(hwnd));
        if (!app_window) {
            return E_FAIL;
        }
        return read_metrics(reinterpret_cast<HWND>(hwnd), app_window.TitleBar(), metrics);
    });
}
