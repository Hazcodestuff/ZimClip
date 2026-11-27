#include <windows.h>
#include <stdio.h>
int main(){HWND h=FindWindowW(L"ZimClipWindow",0);DWORD tid=GetWindowThreadProcessId(h,0);HMODULE m=LoadLibraryW(L"scroll-reentry.dll");HOOKPROC cb=(HOOKPROC)GetProcAddress(m,"Reentry");HHOOK hook=SetWindowsHookExW(WH_CALLWNDPROC,cb,m,tid);printf("hook=%p err=%lu\n",hook,GetLastError());DWORD_PTR res=0;HWND v=GetDlgItem(h,74);LRESULT ok=SendMessageTimeoutW(v,WM_VSCROLL,SB_LINEDOWN,0,SMTO_ABORTIFHUNG,2500,&res);printf("scroll_completed=%lld err=%lu\n",(long long)ok,GetLastError());fflush(stdout);if(hook)UnhookWindowsHookEx(hook);return ok?0:3;}
