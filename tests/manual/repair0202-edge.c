#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
int main(int argc,char**argv){int n=atoi(argv[1]);HWND h=FindWindowW(L"ZimClipWindow",0),c=GetDlgItem(h,78);if(n==0){SendMessageW(c,CB_ADDSTRING,0,(LPARAM)L"AbsentTag");SendMessageW(c,CB_SETCURSEL,SendMessageW(c,CB_GETCOUNT,0,0)-1,0);PostMessageW(h,WM_COMMAND,MAKEWPARAM(78,CBN_SELCHANGE),(LPARAM)c);}if(n==1){wchar_t s[300];GetWindowTextW(c,s,300);printf("filter=%ls\n",s);HWND hint=(HWND)GetPropW(h,L"library-hint");GetWindowTextW(hint,s,300);printf("hint=%ls\n",s);if(wcscmp(s,L"No matches. Clear search or tag filter."))return 3;if(IsWindowVisible(GetDlgItem(h,100)))return 4;}if(n==2){HWND e=FindWindowW(L"ZimClipEditor",0);PostMessageW(e,WM_COMMAND,215,0);}return 0;}
