# 前台键盘布局实时探针：切换输入法/键盘时观察 HKL / ImmIsIME / IME 开关状态
# 运行：uv run python tools/probe_layout.py
import ctypes
import ctypes.wintypes as wt
import sys
import time

sys.stdout.reconfigure(encoding="utf-8")

u32 = ctypes.windll.user32
imm32 = ctypes.windll.imm32
k32 = ctypes.windll.kernel32
imm32.ImmIsIME.argtypes = [wt.HKL]
imm32.ImmIsIME.restype = ctypes.c_int

WM_IME_CONTROL = 0x283
IMC_GETOPENSTATUS = 0x5
IMC_GETCONVERSIONMODE = 0x1


def foreground():
    hwnd = u32.GetForegroundWindow()
    tid = u32.GetWindowThreadProcessId(hwnd, None)
    return hwnd, tid


def ime_ctl(hwnd, imc):
    imm32.ImmGetDefaultIMEWnd.restype = wt.HWND
    imm32.ImmGetDefaultIMEWnd.argtypes = [wt.HWND]
    ime_wnd = imm32.ImmGetDefaultIMEWnd(hwnd)
    if not ime_wnd:
        return None
    result = wt.DWORD()
    res = u32.SendMessageTimeoutW(
        ime_wnd, WM_IME_CONTROL, imc, 0,
        2,  # SMTO_ABORTIFHUNG
        300, ctypes.byref(result),
    )
    return result.value if res else None


def layout_name():
    buf = ctypes.create_unicode_buffer(64)
    if u32.GetKeyboardLayoutNameW(buf):
        return buf.value
    return "?"


def sample():
    hwnd, tid = foreground()
    hkl = u32.GetKeyboardLayout(tid)
    is_ime = imm32.ImmIsIME(hkl)
    lang = hkl & 0xFFFF
    device = (hkl >> 16) & 0xFFFF
    open_status = ime_ctl(hwnd, IMC_GETOPENSTATUS)
    conv = ime_ctl(hwnd, IMC_GETCONVERSIONMODE)
    return (f"hwnd={hwnd & 0xFFFFFFFF:x} tid={tid} hkl={hkl & 0xFFFFFFFF:08x} "
            f"lang={lang:04x} device={device:04x} name={layout_name()} ImmIsIME={bool(is_ime)} "
            f"open={open_status} conv={conv if conv is None else hex(conv)}")


last = ""
print("实时观察（Ctrl+C 退出）：切换 ENG 键盘 / 中文输入法，看各行变化", flush=True)
while True:
    s = sample()
    if s != last:
        last = s
        print(f"{time.strftime('%H:%M:%S')} {s}", flush=True)
    time.sleep(0.15)
