package com.mightymonitor.app;
import android.widget.Button;
import android.widget.CheckBox;
import android.widget.EditText;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.rule.ActivityTestRule;
import androidx.test.platform.app.InstrumentationRegistry;
import org.junit.Rule;
import org.junit.Test;
import org.junit.runner.RunWith;
import static org.junit.Assert.assertTrue;
@RunWith(AndroidJUnit4.class)
public final class ViewerSmokeTest {
    @org.junit.Before public void preserveAccessibilityService() {
        androidx.test.uiautomator.Configurator.getInstance().setUiAutomationFlags(android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES);
        android.app.UiAutomation automation = InstrumentationRegistry.getInstrumentation().getUiAutomation(android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES);
        if ("true".equals(InstrumentationRegistry.getArguments().getString("remoteControlTest"))) {
            // Instrumentation restarts the target app. Rebind its test service afterwards;
            // pre-launch grants can be marked crashed/suppressed by that process restart.
            shell(automation, "settings put secure enabled_accessibility_services null");
            // The enabled-service observer must see a real removal to clear the
            // crashed-service entry left by am instrument restarting this process.
            android.os.SystemClock.sleep(1500);
            shell(automation, "settings put secure enabled_accessibility_services com.mightymonitor.app/.RemoteControlService");
            shell(automation, "settings put secure accessibility_enabled 1");
        }
    }
    private static void shell(android.app.UiAutomation automation, String command) {
        try (java.io.InputStream output = new android.os.ParcelFileDescriptor.AutoCloseInputStream(automation.executeShellCommand(command))) {byte[] buffer = new byte[512]; while (output.read(buffer) >= 0) {}}
        catch (java.io.IOException error) {throw new IllegalStateException("Test system grant failed", error);}
    }
    @Rule public ActivityTestRule<MainActivity> activity = new ActivityTestRule<>(MainActivity.class);
    @Test public void decodesRealDesktopH264ThroughRustProtocol() throws Exception {
        MainActivity app = activity.getActivity();
        long deadline = System.currentTimeMillis() + 60000;
        while (!((Button) app.findViewById(MainActivity.CONNECT_ID)).isEnabled() && System.currentTimeMillis() < deadline) Thread.sleep(100);
        saveScreenshot("view");
        final String pcInvitation = new String(java.nio.file.Files.readAllBytes(new java.io.File(app.getFilesDir(), "pc-invitation").toPath()), java.nio.charset.StandardCharsets.UTF_8).trim();
        InstrumentationRegistry.getInstrumentation().runOnMainSync(() -> {
            ((EditText) app.findViewById(MainActivity.ADDRESS_ID)).setText(pcInvitation);
            app.findViewById(MainActivity.CONNECT_ID).performClick();
        });
        while (app.getRenderedFrames() < 3 && System.currentTimeMillis() < deadline) Thread.sleep(100);
        assertTrue("Android MediaCodec never presented desktop H.264 frames", app.getRenderedFrames() >= 3);
    }

    @Test public void capturesPhoneScreenAndDecodesThroughRustHost() throws Exception {
        MainActivity app = activity.getActivity();
        java.io.File desktopMarker = new java.io.File(app.getFilesDir(), "desktop-video-ok");
        desktopMarker.delete();
        InstrumentationRegistry.getInstrumentation().runOnMainSync(() -> {
            app.findViewById(MainActivity.SHARE_TAB_ID).performClick();
            app.findViewById(MainActivity.SHARE_ID).performClick();
        });
        androidx.test.uiautomator.UiDevice device = androidx.test.uiautomator.UiDevice.getInstance(InstrumentationRegistry.getInstrumentation());
        androidx.test.uiautomator.UiObject2 confirm = device.wait(androidx.test.uiautomator.Until.findObject(androidx.test.uiautomator.By.res("android", "button1")), 20000);
        assertTrue("Android screen capture consent did not appear", confirm != null); confirm.click();
        long client = 0; android.media.MediaCodec decoder = null;
        try {
            long deadline = System.currentTimeMillis() + 60000;
            while (ScreenShareService.capturedFrames < 3 && System.currentTimeMillis() < deadline) Thread.sleep(100);
            assertTrue("MediaProjection produced no encoded screen frames: " + ScreenShareService.summary, ScreenShareService.capturedFrames >= 3);
            saveScreenshot("share");
            client = NativeBridge.connect(ScreenShareService.invitation.replaceFirst("mightymonitor://[^/]+/", "mightymonitor://127.0.0.1:5000/")); int decoded = 0; byte[] pending = null;
            android.media.MediaCodec.BufferInfo info = new android.media.MediaCodec.BufferInfo();
            while (decoded < 3 && System.currentTimeMillis() < deadline) {
                if (decoder != null) { int out = decoder.dequeueOutputBuffer(info, 0); if (out >= 0) { decoder.releaseOutputBuffer(out, false); decoded++; } }
                byte[] event = pending != null ? pending : NativeBridge.poll(client);
                if (event == null) { Thread.sleep(10); continue; }
                if (event[0] == 2) {
                    java.nio.ByteBuffer config = java.nio.ByteBuffer.wrap(event).order(java.nio.ByteOrder.LITTLE_ENDIAN);
                    decoder = android.media.MediaCodec.createDecoderByType("video/avc");
                    android.media.MediaFormat format = android.media.MediaFormat.createVideoFormat("video/avc", config.getInt(1), config.getInt(5));
                    format.setInteger(android.media.MediaFormat.KEY_MAX_INPUT_SIZE, 16 * 1024 * 1024);
                    decoder.configure(format, null, null, 0); decoder.start();
                } else if (event[0] == 3 && decoder != null) {
                    int input = decoder.dequeueInputBuffer(0);
                    if (input < 0) { pending = event; Thread.sleep(10); continue; }
                    java.nio.ByteBuffer buffer = decoder.getInputBuffer(input); buffer.clear(); buffer.put(event, 10, event.length - 10);
                    decoder.queueInputBuffer(input, 0, event.length - 10, java.nio.ByteBuffer.wrap(event).order(java.nio.ByteOrder.LITTLE_ENDIAN).getLong(1), event[9] != 0 ? android.media.MediaCodec.BUFFER_FLAG_KEY_FRAME : 0);
                    pending = null;
                }
            }
            assertTrue("Android capture -> Rust host -> Rust viewer -> decoder produced no video", decoded >= 3);
            NativeBridge.disconnect(client); client = 0;
            java.nio.file.Files.write(new java.io.File(app.getFilesDir(), "share-invitation").toPath(), ScreenShareService.invitation.getBytes(java.nio.charset.StandardCharsets.UTF_8));
            if ("true".equals(InstrumentationRegistry.getArguments().getString("desktopInterop"))) {
                java.io.File marker = new java.io.File(app.getFilesDir(), "desktop-video-ok");
                long until = System.currentTimeMillis() + 60000;
                while (!marker.isFile() && System.currentTimeMillis() < until) Thread.sleep(100);
                assertTrue("Desktop GStreamer viewer did not decode the phone stream", marker.isFile());
            }
        } finally {
            if (client != 0) NativeBridge.disconnect(client);
            if (decoder != null) { decoder.stop(); decoder.release(); }
            app.stopService(new android.content.Intent(app, ScreenShareService.class));
        }
    }
    @Test public void controlsPhoneOnlyAfterBothGrantsAndStopsOnRevocation() throws Exception {
        MainActivity app = activity.getActivity();
        long deadline = System.currentTimeMillis() + 60000;
        while (!RemoteControlService.available() && System.currentTimeMillis() < deadline) Thread.sleep(100);
        assertTrue("Test accessibility grant did not start the service", RemoteControlService.available());
        InstrumentationRegistry.getInstrumentation().runOnMainSync(() -> {
            app.findViewById(MainActivity.SHARE_TAB_ID).performClick();
            ((CheckBox) app.findViewById(MainActivity.SHARE_CONTROL_ID)).setChecked(true);
            app.findViewById(MainActivity.SHARE_ID).performClick();
        });
        androidx.test.uiautomator.UiDevice device = androidx.test.uiautomator.UiDevice.getInstance(InstrumentationRegistry.getInstrumentation());
        androidx.test.uiautomator.UiObject2 confirm = device.wait(androidx.test.uiautomator.Until.findObject(androidx.test.uiautomator.By.res("android", "button1")), 20000);
        assertTrue("Capture consent did not appear", confirm != null); confirm.click();
        while (ScreenShareService.invitation.isEmpty() && System.currentTimeMillis() < deadline) Thread.sleep(100);
        assertTrue("Phone did not start sharing", !ScreenShareService.invitation.isEmpty());
        long client = NativeBridge.connect(ScreenShareService.invitation.replaceFirst("mightymonitor://[^/]+/", "mightymonitor://127.0.0.1:5000/"));
        java.util.concurrent.atomic.AtomicBoolean running = new java.util.concurrent.atomic.AtomicBoolean(true), available = new java.util.concurrent.atomic.AtomicBoolean(), controlling = new java.util.concurrent.atomic.AtomicBoolean();
        Thread pump = new Thread(() -> {while (running.get()) {byte[] event = NativeBridge.poll(client); if (event != null && event.length == 3 && event[0] == 6) {available.set(event[1] != 0); controlling.set(event[2] != 0);} try {Thread.sleep(5);} catch (InterruptedException stop) {return;}}}); pump.start();
        android.app.Activity target = null;
        try {
            while (!available.get() && System.currentTimeMillis() < deadline) Thread.sleep(20);
            assertTrue("Phone did not advertise the locally enabled control grant", available.get());
            NativeBridge.requestControl(client, true);
            while (!controlling.get() && System.currentTimeMillis() < deadline) Thread.sleep(20);
            assertTrue("Phone rejected control", controlling.get());
            target = InstrumentationRegistry.getInstrumentation().startActivitySync(new android.content.Intent(app, InputTestActivity.class).addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK));
            android.widget.Button button = target.findViewById(InputTestActivity.TAP_ID);
            tap(client, button);
            while (InputTestActivity.taps.get() < 1 && System.currentTimeMillis() < deadline) Thread.sleep(20);
            assertTrue("Remote gesture did not click the actual Android button", InputTestActivity.taps.get() >= 1);
            android.widget.EditText editor = target.findViewById(InputTestActivity.EDITOR_ID); tap(client, editor);
            // A prior activity can still have an input connection while the remote
            // tap is in flight. Wait for this actual editor, not just any IME client.
            while (!RemoteKeyboardService.readyFor(InputTestActivity.EDITOR_ID) && System.currentTimeMillis() < deadline) Thread.sleep(20);
            assertTrue("Remote tap did not focus the test editor and connect its keyboard", RemoteKeyboardService.readyFor(InputTestActivity.EDITOR_ID));
            NativeBridge.typeText(client, "Trinix");
            java.util.concurrent.atomic.AtomicReference<String> text = new java.util.concurrent.atomic.AtomicReference<>("");
            while (!"Trinix".equals(text.get()) && System.currentTimeMillis() < deadline) {InstrumentationRegistry.getInstrumentation().runOnMainSync(() -> text.set(editor.getText().toString())); Thread.sleep(20);}
            assertTrue("Remote keyboard did not type into the actual Android editor: " + text.get(), "Trinix".equals(text.get()));
            ScreenShareService.revokeControl();
            while (controlling.get() && System.currentTimeMillis() < deadline) Thread.sleep(20);
            assertTrue("Revoked phone control stayed active", !controlling.get());
            boolean rejected = false; try {NativeBridge.typeText(client, "forbidden");} catch (IllegalStateException expected) {rejected = true;}
            assertTrue("Native client accepted typing after control revocation", rejected);
        } finally {
            running.set(false); pump.join(1000); NativeBridge.disconnect(client);
            app.stopService(new android.content.Intent(app, ScreenShareService.class));
            if (target != null) {android.app.Activity closing = target; InstrumentationRegistry.getInstrumentation().runOnMainSync(closing::finish);}
        }
    }
    private void tap(long client, android.view.View view) {
        int[] location = new int[2]; java.util.concurrent.atomic.AtomicReference<float[]> point = new java.util.concurrent.atomic.AtomicReference<>();
        InstrumentationRegistry.getInstrumentation().runOnMainSync(() -> {
            view.getLocationOnScreen(location); android.util.DisplayMetrics metrics = new android.util.DisplayMetrics(); activity.getActivity().getWindowManager().getDefaultDisplay().getRealMetrics(metrics);
            point.set(new float[]{(location[0] + view.getWidth() / 2f) / metrics.widthPixels, (location[1] + view.getHeight() / 2f) / metrics.heightPixels});
        });
        NativeBridge.pointer(client, point.get()[0], point.get()[1], 0); NativeBridge.pointer(client, point.get()[0], point.get()[1], 2);
    }
    private void saveScreenshot(String name) throws Exception {
        InstrumentationRegistry.getInstrumentation().waitForIdleSync();
        android.graphics.Bitmap bitmap = InstrumentationRegistry.getInstrumentation().getUiAutomation(android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES).takeScreenshot();
        java.io.File directory = new java.io.File(activity.getActivity().getFilesDir(), "screenshots"); directory.mkdirs();
        try (java.io.FileOutputStream output = new java.io.FileOutputStream(new java.io.File(directory, name + ".png"))) { bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, output); }
        bitmap.recycle();
    }
}
