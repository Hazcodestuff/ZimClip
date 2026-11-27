Settings re-entrant paint regression, 0.19.2
Compile DLL and test executables with a Windows x64 C compiler. Run ZimClip
and open Settings, then run scroll-reentry-test.exe beside scroll-reentry.dll.
The hook forces scrollbar UpdateWindow during every Settings child movement.
0.19.1 times out with error1460;0.19.2 completes. scroll-stress-test.exe then
runs40layout transitions and320wheel events under the same hook. This is a
test-only DLL, never include it with app runtime files. It emulates a valid
nested Windows repaint that compatibility tests previously did not produce.

0.20.2 manual audit probes
functional-audit0202.rs and preparation0202-check.rs are Cargo binary probes.
Copy a probe to src/bin/<probe-name>.rs, run cargo build --release --target
x86_64-pc-windows-gnu --bin <probe-name>, and run its EXE from a disposable
runtime test folder containing the real media-export.exe and generated inputs.
Remove the temporary src/bin copy afterward. Do not ship mock helpers or probes.
The export probe expects Clips/wave-lanes-fixture.mp4 with four named audio
tracks; preparation probe expects Clips/preparation30-fixture.mp4, 30 seconds.
Both remove their own generated outputs and compare source bytes on cancel.
They do not validate hardware capture or native MFPlay.

Library layout visibility, 0.20.2
Compile library0202-visibility.c as a Windows x64 console program. With clips
present, test cold starts and Settings > layout > Back in all four layouts.
Also programmatically select Compact then Wide/Stacked while Settings is
hidden. Rows and library hint must return; size toggle returns except in
Stacked, where it is intentionally hidden. Compact hides the entire library.
Inspect Compact footer with Details both closed and open.

back0202-pointer.c clicks the current screen rectangle of Settings/Back using
real mouse down/up. Run once to enter Settings, select a layout, then run
again to return to replay. Re-read the rectangle after every layout change.

0.21.0 category/welcome checks
Compile welcome0210.c, welcome0210-edge.c and categories0210.c as Win32
console test helpers. Welcome navigation IDs are1001Next,1002Skip,1003Back.
The test helper accepts1Next,2Skip,3Back,4set layout,5set valid comms.
Check combo notifications do not navigate; invalid comms blocks Next but not
Back; Skip leaves existing preferences byte-identical and writes only the
welcome marker; Done saves all chosen values and is skipped on later starts.
Run tests from a disposable copy. Never delete a user's welcome marker to test.
For categories, select82Capture,83Appearance,84Files/app and assert the other
pages' controls are hidden. Capture includes keybind/comms/devices. Scroll
bottom in each layout. Quick replay buttons86/87 route to existing System/Mic
capture actions, including save/record/startup guards. Check actual pointer
clicks and persisted settings, not just helper posting success.
