#include <windows.h>
#include <stdio.h>
int main(){HWND h=FindWindowW(L"ZimClipWindow",0);SetWindowTextW(GetDlgItem(h,76),L"refresh0202");Sleep(300);CopyFileW(L"Clips\\card-test-1.mp4",L"Clips\\refresh0202.mp4",0);SendMessageW(GetDlgItem(h,81),BM_CLICK,0,0);Sleep(300);wchar_t t[200];GetWindowTextW(GetDlgItem(h,100),t,200);printf("after refresh card=%ls visible=%d\n",t,IsWindowVisible(GetDlgItem(h,100)));return 0;}
