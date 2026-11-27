# -*- coding: utf-8 -*-
"""IME 中英切换测试：两个按钮，延迟 3 秒后对当时的前台焦点窗口生效。
复刻 rust_indicator/src/ime_switcher.rs 的 press 逻辑，用于测豆包等第三方输入法。
"""
import ctypes
import tkinter as tk
from ctypes import wintypes, Structure, sizeof, POINTER, byref

# 告诉操作系统使用程序自身的 dpi 适配
ctypes.windll.shcore.SetProcessDpiAwareness(1)

WM_IME_CONTROL = 0x283
IMC_GETCONVERSIONMODE = 0x1
IMC_SETCONVERSIONMODE = 0x2
IMC_SETOPENSTATUS = 0x6
SMTO_ABORTIFHUNG = 0x0002

user32 = ctypes.windll.user32
imm32 = ctypes.windll.imm32

user32.SendMessageTimeoutW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM,
                                       wintypes.LPARAM, wintypes.UINT, wintypes.UINT,
                                       POINTER(wintypes.DWORD)]
user32.SendMessageTimeoutW.restype = ctypes.c_longlong
imm32.ImmGetDefaultIMEWnd.argtypes = [wintypes.HWND]
imm32.ImmGetDefaultIMEWnd.restype = wintypes.HWND


class GUITHREADINFO(Structure):
    _fields_ = [("cbSize", wintypes.DWORD), ("flags", wintypes.DWORD),
                ("hwndActive", wintypes.HWND), ("hwndFocus", wintypes.HWND),
                ("hwndCapture", wintypes.HWND), ("hwndMenuOwner", wintypes.HWND),
                ("hwndMoveSize", wintypes.HWND), ("hwndCaret", wintypes.HWND),
                ("rcCaret", wintypes.RECT)]


def focused_window():
    fore = user32.GetForegroundWindow()
    if not fore:
        return None
    tid = user32.GetWindowThreadProcessId(fore, None)
    info = GUITHREADINFO()
    info.cbSize = sizeof(GUITHREADINFO)
    if user32.GetGUIThreadInfo(tid, byref(info)) and info.hwndFocus:
        return info.hwndFocus
    return fore


def ime_ctl(hwnd, imc, value):
    ime_wnd = imm32.ImmGetDefaultIMEWnd(hwnd)
    if not ime_wnd:
        return None
    result = wintypes.DWORD(0)
    ok = user32.SendMessageTimeoutW(ime_wnd, WM_IME_CONTROL, imc, value,
                                    SMTO_ABORTIFHUNG, 600, byref(result))
    return result.value if ok else None


def press(hwnd, chinese):
    # 中文：open+conv 一起压（新版微软拼音会忽略单发 SETOPENSTATUS(1)，缺一不可）；
    # 英文：只压 conv 回非中文模式（Shift 的语义），open 不动
    if chinese:
        ime_ctl(hwnd, IMC_SETOPENSTATUS, 1)
    conv = ime_ctl(hwnd, IMC_GETCONVERSIONMODE, 0) or 0
    mode = (conv & ~3) | 1 if chinese else conv & ~3
    ime_ctl(hwnd, IMC_SETCONVERSIONMODE, mode)


def press_open(hwnd, chinese):
    # imeguard 的结论：对 TSF 输入法 openstatus 才是真开关（关 IME = 英文直通）
    ime_ctl(hwnd, IMC_SETOPENSTATUS, 1 if chinese else 0)
    if chinese:
        conv = ime_ctl(hwnd, IMC_GETCONVERSIONMODE, 0) or 0
        ime_ctl(hwnd, IMC_SETCONVERSIONMODE, (conv & ~3) | 1)


def press_broadcast(hwnd, chinese):
    # imeguard 的进程级广播：对目标进程内所有 class="IME" 窗口各压一遍 open+conv
    pid = wintypes.DWORD(0)
    user32.GetWindowThreadProcessId(hwnd, byref(pid))
    found = []

    @WNDENUMPROC
    def cb(w, _):
        p = wintypes.DWORD(0)
        user32.GetWindowThreadProcessId(w, byref(p))
        if p.value == pid.value and get_class(w) == "IME":
            found.append(w)
        return True

    user32.EnumWindows(cb, None)
    for w in found:
        press(w, chinese)
    return f"广播 {len(found)} 个 IME 窗口"


def press_shift(hwnd, chinese):
    # 模拟按一下 Shift（各中文输入法默认的中英切换热键），只切换不定向
    user32.keybd_event(0x10, 0, 0, 0)
    user32.keybd_event(0x10, 0, 2, 0)
    return "Shift 已发送"


def get_class(h):
    buf = ctypes.create_unicode_buffer(256)
    user32.GetClassNameW(h, buf, 256)
    return buf.value


WNDENUMPROC = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
user32.GetClassNameW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]


class App:
    def __init__(self):
        self.root = tk.Tk()
        self.root.title("IME 切换测试")
        self.root.attributes("-topmost", True)
        # 缩放因子
        scale = ctypes.windll.shcore.GetScaleFactorForDevice(0)
        self.root.tk.call("tk", "scaling", scale / 75)

        self.status = tk.Label(self.root, text="点击后 3 秒内点到目标窗口", font=("微软雅黑", 11))
        self.status.pack(padx=16, pady=8)
        # 各机制逐个对豆包试：conv / openstatus / 进程广播 / Shift 模拟
        actions = [
            ("切中文", True, press),
            ("切英文·conv", False, press),
            ("切英文·关IME", False, press_open),
            ("切中文·广播", True, press_broadcast),
            ("切英文·广播", False, press_broadcast),
            ("切英文·Shift", False, press_shift),
        ]
        for name, cn, fn in actions:
            tk.Button(self.root, text=name, width=14, font=("微软雅黑", 10),
                      command=lambda cn=cn, fn=fn: self.fire(cn, fn)).pack(padx=16, pady=3)

    def fire(self, chinese, fn):
        target = "中文" if chinese else "英文"
        self.status.config(text=f"3 秒后[{fn.__name__}]切{target}，请点目标窗口…")
        self.root.after(3000, lambda: self.apply(chinese, fn, target))

    def apply(self, chinese, fn, target):
        hwnd = focused_window()
        note = fn(hwnd, chinese) if hwnd else "无前台窗口"
        self.status.config(text=f"已[{fn.__name__}]切{target}｜{note}")


if __name__ == "__main__":
    App().root.mainloop()
