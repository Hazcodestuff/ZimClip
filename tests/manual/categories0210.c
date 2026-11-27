#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
int main(int argc,char**argv){HWND h=FindWindowW(L"ZimClipWindow",0),v=GetDlgItem(h,74);int n=atoi(argv[1]);if(n<3)SendMessageW(h,WM_COMMAND,82+n,0);if(n==3)SendMessageW(h,WM_COMMAND,86,0);if(n==4){for(int i=0;i<20;i++)SendMessageW(v,WM_MOUSEWHEEL,MAKEWPARAM(0,-120),0);}Sleep(300);int ids[]={7,12,20,31,45,73,79,80,33,32,86,87};for(int i=0;i<12;i++){HWND c=GetDlgItem(v,ids[i]);if(!c)c=GetDlgItem(h,ids[i]);RECT r;GetWindowRect(c,&r);printf("id=%d visible=%d top=%ld bottom=%ld\n",ids[i],IsWindowVisible(c),r.top,r.bottom);}return 0;}
