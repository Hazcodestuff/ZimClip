#include <windows.h>
#include <stdio.h>
int main(){HWND h=FindWindowW(L"ZimClipWindow",0),v=GetDlgItem(h,74);SendMessageW(v,WM_VSCROLL,SB_BOTTOM,0);for(int id=79;id<=80;id++){HWND c=GetDlgItem(v,id);RECT r;GetWindowRect(c,&r);printf("id=%d visible=%d rect=%ld,%ld %ldx%ld\n",id,IsWindowVisible(c),r.left,r.top,r.right-r.left,r.bottom-r.top);}return 0;}
