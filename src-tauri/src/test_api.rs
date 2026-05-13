fn main() {
    let _ = windows::Win32::UI::Input::KeyboardAndMouse::RegisterHotKey;
    let _ = windows::Win32::UI::Input::KeyboardAndMouse::UnregisterHotKey;
    let _ = windows::Win32::UI::Input::KeyboardAndMouse::MOD_ALT;
    let _ = windows::Win32::UI::Input::KeyboardAndMouse::MOD_SHIFT;
    let _ = windows::Win32::UI::Input::KeyboardAndMouse::MOD_CONTROL;
    let _ = windows::Win32::UI::Input::KeyboardAndMouse::MOD_WIN;
}