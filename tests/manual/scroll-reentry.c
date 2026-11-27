#include <windows.h>
__declspec(dllexport) LRESULT CALLBACK Reentry(int code,WPARAM w,LPARAM l){static int busy=0;if(code>=0&&!busy){CWPSTRUCT *m=(CWPSTRUCT*)l;if(m->message==WM_WINDOWPOSCHANGED&&GetDlgCtrlID(GetParent(m->hwnd))==74){busy=1;HWND main=GetParent(GetParent(m->hwnd));HWND bar=GetDlgItem(main,75);InvalidateRect(bar,0,0);UpdateWindow(bar);busy=0;}}return CallNextHookEx(0,code,w,l);}
BOOL WINAPI DllMain(HINSTANCE h,DWORD reason,LPVOID p){return TRUE;}
