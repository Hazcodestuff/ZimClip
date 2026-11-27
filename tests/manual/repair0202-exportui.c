#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
BOOL CALLBACK labels(HWND h,LPARAM l){wchar_t t[2000];GetWindowTextW(h,t,2000);if(wcsstr(t,L"Saved sized-")||wcsstr(t,L"Export failed"))wprintf(L"status=%ls\n",t);return 1;}
int main(int argc,char**argv){int n=atoi(argv[1]);HWND e=FindWindowW(L"ZimClipEditor",0);if(n==0){SendMessageW(GetDlgItem(e,208),CB_SETCURSEL,6,0);PostMessageW(e,WM_COMMAND,MAKEWPARAM(208,CBN_SELCHANGE),(LPARAM)GetDlgItem(e,208));SetWindowTextW(GetDlgItem(e,211),L"3");PostMessageW(e,WM_COMMAND,206,0);}else EnumChildWindows(e,labels,0);return 0;}
