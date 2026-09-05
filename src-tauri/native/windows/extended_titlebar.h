#pragma once

#include <stdint.h>

#ifdef _WIN32
#define VERENU_WINDOWS_CHROME_API extern "C" __declspec(dllexport)
#else
#define VERENU_WINDOWS_CHROME_API extern "C"
#endif

struct VerenuTitleBarMetrics {
    int32_t titlebar_height;
    int32_t left_inset;
    int32_t right_inset;
    uint32_t dpi;
    int32_t window_left;
    int32_t window_top;
    int32_t window_right;
    int32_t window_bottom;
    int32_t client_left;
    int32_t client_top;
    int32_t client_right;
    int32_t client_bottom;
    int32_t client_screen_x;
    int32_t client_screen_y;
    int32_t is_maximized;
    int32_t extends_content;
};

VERENU_WINDOWS_CHROME_API int32_t verenu_enable_extended_titlebar(
    intptr_t hwnd,
    int32_t dark,
    VerenuTitleBarMetrics* metrics);

VERENU_WINDOWS_CHROME_API int32_t verenu_update_extended_titlebar(
    intptr_t hwnd,
    int32_t dark,
    VerenuTitleBarMetrics* metrics);

VERENU_WINDOWS_CHROME_API int32_t verenu_get_extended_titlebar_metrics(
    intptr_t hwnd,
    VerenuTitleBarMetrics* metrics);

VERENU_WINDOWS_CHROME_API int32_t verenu_set_runtime_icons(
    intptr_t hwnd,
    intptr_t taskbar_icon,
    intptr_t titlebar_icon,
    const wchar_t* taskbar_icon_path);
