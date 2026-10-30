package com.mightymonitor.app;

import android.app.*;
import android.content.*;
import android.content.pm.ServiceInfo;
import android.graphics.Rect;
import android.hardware.display.DisplayManager;
import android.hardware.display.VirtualDisplay;
import android.media.*;
import android.media.projection.*;
import android.os.*;
import android.util.DisplayMetrics;
import android.view.*;
import java.io.ByteArrayOutputStream;
import java.net.*;
import java.nio.ByteBuffer;
import java.util.Collections;

/** Android consent/capture/codec adapter. Rust owns the screen-sharing server. */
public final class ScreenShareService extends Service {
    public static final String STOP = "com.mightymonitor.STOP_SHARING";
    public static volatile boolean sharing, allowControl;
    public static volatile int captureWidth, captureHeight;
    public static volatile String invitation = "";
    public static volatile String summary = "Not sharing";
    public static volatile long capturedFrames;
    private static volatile ScreenShareService current;
    private HandlerThread thread;
    private Handler worker;
    private MediaProjection projection;
    private MediaCodec encoder;
    private VirtualDisplay display;
    private Surface input;
    private long host;
    private byte[] codecHeaders = new byte[0];
    private boolean closed;
    private int bitrate = 4_000_000;
    private long lastKeyRequest;
    private MediaProjection.Callback projectionCallback;
    @Override public void onCreate() { super.onCreate(); current = this; thread = new HandlerThread("mm-screen-capture"); thread.start(); worker = new Handler(thread.getLooper()); }
    @Override public int onStartCommand(Intent intent, int flags, int startId) {
        if (intent == null || STOP.equals(intent.getAction())) { stopSelf(); return START_NOT_STICKY; }
        if (sharing) return START_NOT_STICKY;
        NotificationManager notifications = getSystemService(NotificationManager.class);
        notifications.createNotificationChannel(new NotificationChannel("sharing", "Screen sharing", NotificationManager.IMPORTANCE_LOW));
        PendingIntent open = PendingIntent.getActivity(this, 0, new Intent(this, MainActivity.class), PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        PendingIntent stop = PendingIntent.getService(this, 1, new Intent(this, ScreenShareService.class).setAction(STOP), PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        Notification notification = new Notification.Builder(this, "sharing").setSmallIcon(android.R.drawable.presence_video_online).setContentTitle("Sharing your screen").setContentText("Tap Stop to end sharing").setContentIntent(open).setOngoing(true).addAction(new Notification.Action.Builder(null, "Stop", stop).build()).build();
        if (Build.VERSION.SDK_INT >= 29) startForeground(1, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION); else startForeground(1, notification);
        allowControl = intent.getBooleanExtra("allowControl", false) && RemoteControlService.available();
        sharing = true; summary = "Starting sharing…"; capturedFrames = 0;
        int resultCode = intent.getIntExtra("resultCode", Activity.RESULT_CANCELED);
        Intent permission = intent.getParcelableExtra("permission");
        worker.post(() -> { try { startCapture(resultCode, permission); } catch (Exception | UnsatisfiedLinkError error) { fail(error); } });
        return START_NOT_STICKY;
    }
    private void startCapture(int resultCode, Intent permission) throws Exception {
        if (closed || permission == null || resultCode != Activity.RESULT_OK) throw new IllegalArgumentException("Screen sharing permission was not granted");
        WindowManager windows = getSystemService(WindowManager.class);
        int width, height;
        if (Build.VERSION.SDK_INT >= 30) { Rect bounds = windows.getMaximumWindowMetrics().getBounds(); width = bounds.width(); height = bounds.height(); }
        else { DisplayMetrics metrics = new DisplayMetrics(); windows.getDefaultDisplay().getRealMetrics(metrics); width = metrics.widthPixels; height = metrics.heightPixels; }
        double scale = Math.min(1.0, 1280.0 / Math.max(width, height));
        width = Math.max(2, ((int)(width * scale)) & ~1); height = Math.max(2, ((int)(height * scale)) & ~1);
        encoder = MediaCodec.createEncoderByType(MediaFormat.MIMETYPE_VIDEO_AVC);
        MediaCodecInfo.VideoCapabilities capabilities = encoder.getCodecInfo().getCapabilitiesForType(MediaFormat.MIMETYPE_VIDEO_AVC).getVideoCapabilities();
        int widthAlignment = Math.max(2, capabilities.getWidthAlignment());
        int heightAlignment = Math.max(2, capabilities.getHeightAlignment());
        width = Math.max(widthAlignment, width / widthAlignment * widthAlignment);
        height = Math.max(heightAlignment, height / heightAlignment * heightAlignment);
        int fps = 30;
        // Capability metadata can be incomplete on older codec implementations.
        // Honor available rate limits; configure() is the final compatibility check.
        if (capabilities.isSizeSupported(width, height)) fps = Math.max(1, Math.min(30, (int)Math.floor(capabilities.getSupportedFrameRatesFor(width, height).getUpper())));
        android.util.Log.i("Trinix", "Capture " + width + "x" + height + " at " + fps + " fps");
        captureWidth = width; captureHeight = height;
        host = NativeBridge.startHost("0.0.0.0:5000", width, height, fps);
        invitation = NativeBridge.hostInvite(host, localAddress());
        NativeBridge.allowHostControl(host, allowControl);
        worker.post(drainInput);
        encoder.setCallback(new MediaCodec.Callback() {
            @Override public void onInputBufferAvailable(MediaCodec codec, int index) { }
            @Override public void onOutputBufferAvailable(MediaCodec codec, int index, MediaCodec.BufferInfo info) {
                if (closed) return;
                try {
                    ByteBuffer output = codec.getOutputBuffer(index);
                    if (output != null && info.size > 0) {
                        output.position(info.offset); output.limit(info.offset + info.size);
                        byte[] bytes = new byte[info.size]; output.get(bytes);
                        if ((info.flags & MediaCodec.BUFFER_FLAG_CODEC_CONFIG) != 0) codecHeaders = annexB(bytes);
                        else {
                            boolean key = (info.flags & MediaCodec.BUFFER_FLAG_KEY_FRAME) != 0;
                            byte[] accessUnit = annexB(bytes);
                            if (key && codecHeaders.length > 0) { byte[] combined = new byte[codecHeaders.length + accessUnit.length]; System.arraycopy(codecHeaders, 0, combined, 0, codecHeaders.length); System.arraycopy(accessUnit, 0, combined, codecHeaders.length, accessUnit.length); accessUnit = combined; }
                            NativeBridge.publish(host, accessUnit, Math.max(0, info.presentationTimeUs), key); capturedFrames++;
                        }
                    }
                    codec.releaseOutputBuffer(index, false);
                } catch (Exception error) { fail(error); }
            }
            @Override public void onOutputFormatChanged(MediaCodec codec, MediaFormat format) {
                try {
                    ByteArrayOutputStream headers = new ByteArrayOutputStream();
                    for (String key : new String[]{"csd-0", "csd-1"}) { ByteBuffer value = format.getByteBuffer(key); if (value != null) { byte[] data = new byte[value.remaining()]; value.duplicate().get(data); headers.write(annexB(data)); } }
                    codecHeaders = headers.toByteArray();
                } catch (Exception error) { fail(error); }
            }
            @Override public void onError(MediaCodec codec, MediaCodec.CodecException error) { fail(error); }
        }, worker);
        MediaFormat format = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, width, height);
        format.setInteger(MediaFormat.KEY_COLOR_FORMAT, MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface);
        format.setInteger(MediaFormat.KEY_BIT_RATE, bitrate); format.setInteger(MediaFormat.KEY_FRAME_RATE, fps); format.setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 1);
        format.setLong(MediaFormat.KEY_REPEAT_PREVIOUS_FRAME_AFTER, 1_000_000L / fps);
        encoder.configure(format, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE);
        input = encoder.createInputSurface(); encoder.start();
        projection = getSystemService(MediaProjectionManager.class).getMediaProjection(resultCode, permission);
        projectionCallback = new MediaProjection.Callback() {
            @Override public void onStop() { summary = "Sharing stopped"; stopSelf(); }
        };
        projection.registerCallback(projectionCallback, worker);
        display = projection.createVirtualDisplay("Trinix", width, height, getResources().getConfiguration().densityDpi, DisplayManager.VIRTUAL_DISPLAY_FLAG_AUTO_MIRROR, input, null, worker);
        worker.post(update);
    }
    private final Runnable update = new Runnable() {
        @Override public void run() {
            if (closed || host == 0) return;
            try {
                summary = NativeBridge.hostStatus(host);
                long now = SystemClock.elapsedRealtime();
                Bundle parameters = new Bundle(); boolean change = false;
                if (NativeBridge.hostNeedsKeyframe(host) && now - lastKeyRequest > 500) { parameters.putInt(MediaCodec.PARAMETER_KEY_REQUEST_SYNC_FRAME, 0); lastKeyRequest = now; change = true; }
                int requested = NativeBridge.hostBitrate(host);
                if (requested != bitrate) { parameters.putInt(MediaCodec.PARAMETER_KEY_VIDEO_BITRATE, requested); bitrate = requested; change = true; }
                if (change) encoder.setParameters(parameters);
                worker.postDelayed(this, 200);
            } catch (Exception error) { fail(error); }
        }
    };
    private void fail(Throwable error) { summary = "Sharing stopped: " + error.getMessage(); stopSelf(); }
    static byte[] annexB(byte[] bytes) throws Exception {
        if (bytes.length >= 4 && bytes[0] == 0 && bytes[1] == 0 && (bytes[2] == 1 || (bytes[2] == 0 && bytes[3] == 1))) return bytes;
        ByteArrayOutputStream output = new ByteArrayOutputStream(); int offset = 0;
        while (offset < bytes.length) {
            if (bytes.length - offset < 4) throw new IllegalArgumentException("Incomplete H.264 access unit");
            long length = ((bytes[offset] & 255L) << 24) | ((bytes[offset+1] & 255L) << 16) | ((bytes[offset+2] & 255L) << 8) | (bytes[offset+3] & 255L); offset += 4;
            if (length == 0 || length > bytes.length - offset) throw new IllegalArgumentException("Invalid H.264 access unit");
            output.write(new byte[]{0,0,0,1}); output.write(bytes, offset, (int)length); offset += (int)length;
        }
        return output.toByteArray();
    }
    public static void revokeControl() {
        allowControl = false; RemoteControlService.stopInput();
        ScreenShareService service = current;
        if (service != null) service.worker.post(() -> {if (service.host != 0) NativeBridge.allowHostControl(service.host, false);});
    }
    private final Runnable drainInput = new Runnable() {
        @Override public void run() {
            if (closed || host == 0) return;
            if (allowControl && !RemoteControlService.available()) {allowControl = false; NativeBridge.allowHostControl(host, false);}
            for (int i = 0; i < 32; i++) {byte[] input = NativeBridge.pollHostInput(host); if (input == null) break; RemoteControlService.accept(input);}
            worker.postDelayed(this, 16);
        }
    };
    public static String localAddress() {
        try { for (NetworkInterface network : Collections.list(NetworkInterface.getNetworkInterfaces())) for (InetAddress address : Collections.list(network.getInetAddresses())) if (address instanceof Inet4Address && address.isSiteLocalAddress() && !address.isLoopbackAddress()) return address.getHostAddress() + ":5000"; } catch (Exception ignored) { }
        return "Connect to Wi-Fi to see your address";
    }
    @Override public void onDestroy() {
        if (current == this) current = null;
        sharing = false; allowControl = false; invitation = ""; RemoteControlService.stopInput();
        worker.post(() -> {
            closed = true; worker.removeCallbacks(update); worker.removeCallbacks(drainInput);
            if (projection != null && projectionCallback != null) projection.unregisterCallback(projectionCallback);
            if (display != null) display.release();
            if (encoder != null) { try { encoder.stop(); } catch (Exception ignored) {} encoder.release(); }
            if (input != null) input.release();
            if (projection != null) projection.stop();
            if (host != 0) { NativeBridge.stopHost(host); host = 0; }
            thread.quitSafely();
        });
        stopForeground(true); super.onDestroy();
    }
    @Override public IBinder onBind(Intent intent) { return null; }
}
