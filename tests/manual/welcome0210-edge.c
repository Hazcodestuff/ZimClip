#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
int main(int argc,char**argv){HWND h=FindWindowW(L"ZimClipWelcome",0);int n=atoi(argv[1]);if(!h)return 2;if(n==0){wchar_t b[200];HWND c=GetWindow(h,GW_CHILD);GetWindowTextW(c,b,200);printf("first=%ls\n",b);}if(n==1){HWND c=FindWindowExW(h,0,L"COMBOBOX",0);SendMessageW(c,CB_SETCURSEL,3,0);SendMessageW(h,WM_COMMAND,MAKEWPARAM(101,CBN_SELCHANGE),(LPARAM)c);}if(n==2){HWND c=FindWindowExW(h,0,L"EDIT",0);SetWindowTextW(c,L"C:\\bad path\\discord.exe");}return 0;}
