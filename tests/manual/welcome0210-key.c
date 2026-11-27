#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
int main(int argc,char **argv){HWND h=FindWindowW(L"ZimClipWelcome",0);int n=argc>1?atoi(argv[1]):0; if(n==3){if(!RegisterHotKey(0,97,MOD_CONTROL|MOD_SHIFT|MOD_NOREPEAT,'Y'))return 5;Sleep(8000);UnregisterHotKey(0,97);return 0;}if(!h)return 2;HWND c=FindWindowExW(h,0,L"msctls_hotkey32",0);if(!c)return 4;if(n==1)SendMessageW(c,0x401,'A',0);if(n==2)SendMessageW(c,0x401,'Y'|(3<<8),0);if(n==4)SendMessageW(c,0x401,'G'|(4<<8),0);return 0;}
