# 鼠标光标句柄探针：实时打印当前光标句柄 + 前台进程，只记变化
# 用法：uv run python tools/probe_cursor_handle.py
# 依次把鼠标放进去对比：PS 画布(文字工具各状态) / PS 面板 / 普通程序，看句柄值
import ctypes
import ctypes.wintypes as wt
import sys
import time

sys.stdout.reconfigure(encoding="utf-8")

u32 = ctypes.windll.user32
k32 = ctypes.windll.kernel32


class CURSORINFO(ctypes.Structure):
    _fields_ = [("cbSize", wt.DWORD), ("flags", wt.DWORD),
                ("hCursor", wt.HANDLE), ("ptScreenPos", wt.POINT)]


def foreground_tag():
    hwnd = u32.GetForegroundWindow()
    pid = wt.DWORD()
    u32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    name = "?"
    PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
    h = k32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, pid)
    if h:
        buf = ctypes.create_unicode_buffer(512)
        size = wt.DWORD(512 * 2)
        if k32.QueryFullProcessImageNameW(h, 0, buf, ctypes.byref(size)):
            name = buf.value.rsplit("\\", 1)[-1]
        k32.CloseHandle(h)
    return f"{name}#{pid.value}"


u32.GetCursorInfo.argtypes = [ctypes.POINTER(CURSORINFO)]
last = ""
deadline = time.time() + 60
print("60 秒窗口：把鼠标依次放进 PS 画布(文字工具)、PS 面板、PS 外普通程序，看句柄变化", flush=True)
while time.time() < deadline:
    ci = CURSORINFO()
    ci.cbSize = ctypes.sizeof(CURSORINFO)
    if u32.GetCursorInfo(ctypes.byref(ci)):
        state = f"hCursor={ci.hCursor:#x} pos=({ci.ptScreenPos.x},{ci.ptScreenPos.y}) {foreground_tag()}"
        if state != last:
            last = state
            print(f"{time.strftime('%H:%M:%S')} {state}", flush=True)
    time.sleep(0.1)
print("end", flush=True)
