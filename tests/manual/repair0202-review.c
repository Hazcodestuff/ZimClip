#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
int main(int argc,char**argv){int n=atoi(argv[1]);HWND h=FindWindowW(L"ZimClipWindow",0),e=FindWindowW(L"ZimClipEditor",0);if(n<3)PostMessageW(h,WM_COMMAND,82+n,0);if(n==3){SendMessageW(GetDlgItem(e,208),CB_SETCURSEL,6,0);PostMessageW(e,WM_COMMAND,MAKEWPARAM(208,CBN_SELCHANGE),(LPARAM)GetDlgItem(e,208));}if(n==4){printf("custom_visible=%d\n",IsWindowVisible(GetDlgItem(e,211)));}return 0;}
