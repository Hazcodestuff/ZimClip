#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
int main(int argc,char**argv){int n=atoi(argv[1]);HWND h=FindWindowW(L"ZimClipDiagnostics",0);if(n==0)PostMessageW(FindWindowW(L"ZimClipWindow",0),WM_COMMAND,5,0);if(n==1){if(!h)return 8;SendMessageW(GetDlgItem(h,401),BM_CLICK,0,0);}if(n==2){if(!OpenClipboard(h))return 9;HANDLE c=GetClipboardData(CF_UNICODETEXT);wchar_t*p=GlobalLock(c);int ok=p&&wcsstr(p,L"CAPTURE LOG")&&wcsstr(p,L"WORKBENCH LOG");printf("clipboard_report=%d\n",ok);if(p)GlobalUnlock(c);CloseClipboard();if(!ok)return 3;}if(n==3)PostMessageW(h,WM_COMMAND,403,0);return 0;}
