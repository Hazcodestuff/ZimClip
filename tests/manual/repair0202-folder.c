#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
int main(int argc,char**argv){int n=atoi(argv[1]);HWND h=FindWindowW(L"ZimClipWindow",0);if(n==0)PostMessageW(h,WM_COMMAND,80,0);if(n==1){HWND d=FindWindowW(L"#32770",0);printf("dialog=%p\n",d);SetForegroundWindow(d);keybd_event(VK_MENU,0,0,0);keybd_event('D',0,0,0);keybd_event('D',0,KEYEVENTF_KEYUP,0);keybd_event(VK_MENU,0,KEYEVENTF_KEYUP,0);Sleep(150);const wchar_t*s=L"C:\\zimclip-folder-test";for(;*s;s++){INPUT k={0};k.type=INPUT_KEYBOARD;k.ki.wScan=*s;k.ki.dwFlags=KEYEVENTF_UNICODE;SendInput(1,&k,sizeof(k));k.ki.dwFlags|=KEYEVENTF_KEYUP;SendInput(1,&k,sizeof(k));}keybd_event(VK_RETURN,0,0,0);keybd_event(VK_RETURN,0,KEYEVENTF_KEYUP,0);}if(n==2){HWND d=FindWindowW(L"#32770",0);SendMessageW(d,WM_COMMAND,IDOK,0);}return 0;}
