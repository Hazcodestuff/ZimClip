#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
int main(int argc,char**argv){HWND h=FindWindowW(L"ZimClipWindow",0);for(int id=100;id<105;id++){HWND r=GetDlgItem(h,id);RECT b;wchar_t t[100];GetWindowRect(r,&b);GetWindowTextW(r,t,100);printf("id=%d visible=%d style=%lx rect=%ld,%ld,%ld,%ld label=%ls\n",id,IsWindowVisible(r),GetWindowLongW(r,GWL_STYLE),b.left,b.top,b.right,b.bottom,t);}for(int id=36;id<=37;id++)printf("toolbar=%d visible=%d\n",id,IsWindowVisible(GetDlgItem(h,id)));if(argc>1)SendMessageW(h,WM_COMMAND,81,0);return 0;}
