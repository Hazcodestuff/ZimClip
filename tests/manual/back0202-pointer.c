#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
int main(int argc,char**argv){HWND h=FindWindowW(L"ZimClipWindow",0),b=GetDlgItem(h,14);RECT r;GetWindowRect(b,&r);wchar_t t[80];GetWindowTextW(b,t,80);printf("before=%ls visible=%d enabled=%d rect=%ld,%ld,%ld,%ld\n",t,IsWindowVisible(b),IsWindowEnabled(b),r.left,r.top,r.right,r.bottom);SetForegroundWindow(h);SetCursorPos((r.left+r.right)/2,(r.top+r.bottom)/2);Sleep(200);mouse_event(MOUSEEVENTF_LEFTDOWN,0,0,0,0);Sleep(50);mouse_event(MOUSEEVENTF_LEFTUP,0,0,0,0);Sleep(500);GetWindowTextW(b,t,80);printf("after=%ls row=%d\n",t,IsWindowVisible(GetDlgItem(h,100)));return 0;}
