#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
int main(int argc,char**argv){HWND h=FindWindowW(L"ZimClipWelcome",0);int n=argc>1?atoi(argv[1]):0;if(!h){printf("no welcome\n");return 2;}if(n==0){wchar_t b[200];GetWindowTextW(GetDlgItem(h,0),b,200);printf("welcome exists\n");}if(n==1||n==2||n==3)SendMessageW(h,WM_COMMAND,1000+n,0);if(n==4){HWND c=FindWindowExW(h,0,L"COMBOBOX",0);SendMessageW(c,CB_SETCURSEL,2,0);}if(n==5){HWND c=FindWindowExW(h,0,L"EDIT",0);SetWindowTextW(c,L"discord, teams");}return 0;}
