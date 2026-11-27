#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
int main(int argc,char**argv){int n=atoi(argv[1]);HWND h=FindWindowW(L"ZimClipAudioDevices",0);if(n==0){HWND main=FindWindowW(L"ZimClipWindow",0);PostMessageW(main,WM_COMMAND,79,0);}if(n==1){printf("mic_enabled=%d system_enabled=%d\n",IsWindowEnabled(GetDlgItem(h,460)),IsWindowEnabled(GetDlgItem(h,461)));}if(n==2)SendMessageW(GetDlgItem(h,402),BM_CLICK,0,0);return 0;}
