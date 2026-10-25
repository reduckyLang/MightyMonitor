package com.mightymonitor.app;

import android.app.Activity;
import android.Manifest;
import android.graphics.Color;
import android.media.MediaCodec;
import android.media.MediaFormat;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.text.InputType;
import android.view.Gravity;
import android.view.SurfaceHolder;
import android.view.SurfaceView;
import android.view.View;
import android.widget.Button;
import android.widget.CheckBox;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.TextView;
import android.content.pm.PackageManager;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.atomic.AtomicInteger;

/** Platform UI and video decoding only. The screen-sharing protocol lives in Rust. */
public final class MainActivity extends Activity implements SurfaceHolder.Callback {
    public static final int ADDRESS_ID = 101, TRUST_ID = 102, CONNECT_ID = 103, SHARE_TAB_ID = 104, SHARE_TRUST_ID = 105, SHARE_ID = 106, CONTROL_ID = 107, SHARE_CONTROL_ID = 108, VIDEO_ID = 109;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private final ExecutorService cleanup = Executors.newSingleThreadExecutor();
    private final AtomicInteger renderedFrames = new AtomicInteger();
    private SurfaceView video;
    private TextView status;
    private TextView emptyPreview;
    private TextView ownAddress, wifiBanner;
    private EditText address;
    private CheckBox trusted, viewControl, shareControl;
    private Button keyboard, details;
    private boolean updatingControl, remoteControlActive, detailsExpanded, notificationPermissionDenied;
    public boolean isRemoteControlActive() {return remoteControlActive;}
    private Button connect;
    private MediaCodec codec;
    private long handle;
    private boolean surfaceReady;
    private byte[] pendingFrame;
    private int videoWidth, videoHeight;
    private final MediaCodec.BufferInfo bufferInfo = new MediaCodec.BufferInfo();
    private final Runnable pump = new Runnable() {
        @Override public void run() {
            if (handle == 0) return;
            try {
                drainOutput();
                for (int count = 0; count < 4; count++) {
                    if (pendingFrame != null) {
                        if (!queueFrame(pendingFrame)) break;
                        pendingFrame = null;
                    }
                    byte[] event = NativeBridge.poll(handle);
                    if (event == null) break;
                    consume(event);
                    drainOutput();
                }
            } catch (Exception error) {
                android.util.Log.w("MightyMonitor", "Video stopped", error);
                disconnect(ErrorText.plain(error));
                return;
            }
            handler.postDelayed(this, 10);
        }
    };
    private LinearLayout viewPage, sharePage;
    private TextView shareStatus;
    private Button share;
    private final Runnable refreshSharing = new Runnable() {
        @Override public void run() {
            share.setText(ScreenShareService.sharing ? R.string.stop_sharing : R.string.share_this_screen);
            if (notificationPermissionDenied && ScreenShareService.sharing) shareStatus.setText(R.string.notification_denied);
            else shareStatus.setText(ScreenShareService.summary);
            shareControl.setEnabled(!ScreenShareService.sharing || ScreenShareService.allowControl);
            if (ScreenShareService.sharing && !ScreenShareService.allowControl) shareControl.setChecked(false);
            String localAddress = ScreenShareService.localAddress();
            ownAddress.setText(localAddress == null ? "" : localAddress);
            ownAddress.setVisibility(localAddress != null && detailsExpanded ? View.VISIBLE : View.GONE);
            details.setVisibility(localAddress == null ? View.GONE : View.VISIBLE);
            wifiBanner.setVisibility(localAddress == null ? View.VISIBLE : View.GONE);
            handler.postDelayed(this, 500);
        }
    };
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        notificationPermissionDenied = android.os.Build.VERSION.SDK_INT >= 33
                && getPreferences(MODE_PRIVATE).getBoolean("notification_permission_requested", false)
                && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED;
        LinearLayout layout = column(); layout.setFitsSystemWindows(true);
        layout.setPadding(dp(20), dp(18), dp(20), dp(12));
        layout.setBackgroundColor(Color.rgb(247, 248, 247));
        layout.addView(text(getString(R.string.app_title), 26, Color.rgb(30, 39, 35)));
        LinearLayout navigation = new LinearLayout(this);
        Button viewTab = new Button(this); viewTab.setText(R.string.view_tab);
        Button shareTab = new Button(this); shareTab.setText(R.string.share_tab); shareTab.setId(SHARE_TAB_ID);
        styleTab(viewTab, true); styleTab(shareTab, false);
        navigation.addView(viewTab, new LinearLayout.LayoutParams(0, dp(56), 1));
        navigation.addView(shareTab, new LinearLayout.LayoutParams(0, dp(56), 1)); layout.addView(navigation);
        viewPage = column(); sharePage = column();
        layout.addView(viewPage, new LinearLayout.LayoutParams(-1, 0, 1));
        layout.addView(sharePage, new LinearLayout.LayoutParams(-1, 0, 1)); sharePage.setVisibility(View.GONE);
        viewTab.setOnClickListener(v -> { viewPage.setVisibility(View.VISIBLE); sharePage.setVisibility(View.GONE); styleTab(viewTab, true); styleTab(shareTab, false); });
        shareTab.setOnClickListener(v -> { disconnect(getString(R.string.not_connected)); viewPage.setVisibility(View.GONE); sharePage.setVisibility(View.VISIBLE); styleTab(viewTab, false); styleTab(shareTab, true); });
        address = new EditText(this); address.setId(ADDRESS_ID); address.setSingleLine(true);
        address.setHint(R.string.paste_hint);
        address.setFilters(new android.text.InputFilter[]{new android.text.InputFilter.LengthFilter(512)});
        address.setImportantForAutofill(View.IMPORTANT_FOR_AUTOFILL_NO); address.setTextSize(16);
        address.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS);
        getPreferences(MODE_PRIVATE).edit().remove("address").apply();
        viewPage.addView(address, new LinearLayout.LayoutParams(-1, dp(56)));
        if (getResources().getBoolean(R.bool.legacy_toggle)) {
            int legacyLabel = getResources().getIdentifier("adv_legacy_toggle", "string", getPackageName());
            if (legacyLabel == 0) throw new IllegalStateException("Missing development label");
            trusted = new CheckBox(this); trusted.setId(TRUST_ID); trusted.setText(legacyLabel); trusted.setTextSize(14);
            viewPage.addView(trusted);
            trusted.setOnCheckedChangeListener((v, checked) -> { if (!checked) disconnect(getString(R.string.not_connected)); });
        }
        LinearLayout buttons = new LinearLayout(this);
        connect = new Button(this); connect.setId(CONNECT_ID); connect.setText(R.string.connect); connect.setEnabled(false); styleButton(connect, true); connect.setOnClickListener(v -> { if (handle != 0) disconnect(getString(R.string.not_connected)); else startViewing(); });
        buttons.addView(connect, new LinearLayout.LayoutParams(0, -2, 1));
        viewPage.addView(buttons);
        LinearLayout controlBar = new LinearLayout(this);
        viewControl = new CheckBox(this); viewControl.setId(CONTROL_ID); viewControl.setText(R.string.allow_computer_control); viewControl.setVisibility(View.GONE); controlBar.addView(viewControl);
        keyboard = new Button(this); keyboard.setText(R.string.keyboard); styleButton(keyboard, false); keyboard.setVisibility(View.GONE); controlBar.addView(keyboard); viewPage.addView(controlBar);
        viewControl.setOnCheckedChangeListener((v, enabled) -> { if (!updatingControl && handle != 0) { try { NativeBridge.requestControl(handle, enabled); } catch (Exception error) { android.util.Log.w("MightyMonitor", "Could not change control permission", error); disconnect(ErrorText.plain(error)); } } });
        keyboard.setOnClickListener(v -> {
            EditText text = new EditText(this); text.setHint(R.string.type_hint); text.setFilters(new android.text.InputFilter[]{new android.text.InputFilter.LengthFilter(1024)});
            new android.app.AlertDialog.Builder(this).setTitle(R.string.keyboard).setView(text).setPositiveButton(R.string.type, (dialog, which) -> {
                try { if (handle != 0 && text.length() > 0) NativeBridge.typeText(handle, text.getText().toString()); } catch (Exception error) { android.util.Log.w("MightyMonitor", "Could not send typing", error); disconnect(ErrorText.plain(error)); }
            }).setNeutralButton(R.string.enter, (dialog, which) -> { try { if (handle != 0) {NativeBridge.key(handle,40,true);NativeBridge.key(handle,40,false);} } catch (Exception error) { android.util.Log.w("MightyMonitor", "Could not send a key", error); disconnect(ErrorText.plain(error));} }).setNegativeButton(R.string.cancel, null).show();
        });
        status = text(getString(R.string.not_connected), 14, Color.rgb(61, 91, 77)); status.setPadding(0, dp(8), 0, dp(8)); viewPage.addView(status);
        video = new SurfaceView(this); video.setId(VIDEO_ID); video.getHolder().addCallback(this); video.setBackgroundColor(Color.BLACK);
        video.setFocusableInTouchMode(true);
        video.setOnFocusChangeListener((v, focused) -> {if (!focused && handle != 0 && remoteControlActive) {try {NativeBridge.releaseInput(handle);} catch (Exception error) {android.util.Log.w("MightyMonitor", "Could not release control", error); disconnect(ErrorText.plain(error));}}});
        video.setOnTouchListener((v, event) -> {
            if (handle == 0 || !viewControl.isChecked()) return false;
            int action = event.getActionMasked(); int phase;
            if (action == android.view.MotionEvent.ACTION_DOWN) {phase = 0; video.requestFocus();}
            else if (action == android.view.MotionEvent.ACTION_MOVE) phase = 1;
            else if (action == android.view.MotionEvent.ACTION_UP) phase = 2;
            else if (action == android.view.MotionEvent.ACTION_CANCEL) phase = 3;
            else return true;
            try { NativeBridge.pointer(handle, Math.max(0, Math.min(1, event.getX() / Math.max(1, video.getWidth()))), Math.max(0, Math.min(1, event.getY() / Math.max(1, video.getHeight()))), phase); }
            catch (Exception error) { android.util.Log.w("MightyMonitor", "Could not send pointer input", error); disconnect(ErrorText.plain(error)); }
            if (action == android.view.MotionEvent.ACTION_UP) v.performClick();
            return true;
        });
        video.setOnKeyListener((v, code, event) -> {
            if (handle == 0 || !viewControl.isChecked()) return false;
            int hid = androidKey(code); if (hid == 0) return false;
            try { NativeBridge.key(handle, hid, event.getAction() == android.view.KeyEvent.ACTION_DOWN); } catch (Exception error) {android.util.Log.w("MightyMonitor", "Could not send key input", error); disconnect(ErrorText.plain(error));}
            return true;
        });
        video.addOnLayoutChangeListener((v,l,t,r,b,ol,ot,or,ob) -> fitVideo());
        android.widget.FrameLayout viewport = new android.widget.FrameLayout(this);
        viewport.addView(video, new android.widget.FrameLayout.LayoutParams(-1, -1, Gravity.CENTER));
        emptyPreview = text(getString(R.string.empty_preview), 15, Color.rgb(83, 102, 91)); emptyPreview.setGravity(Gravity.CENTER); emptyPreview.setBackground(rounded(Color.rgb(235, 241, 237)));
        viewport.addView(emptyPreview, new android.widget.FrameLayout.LayoutParams(-1, -1)); viewPage.addView(viewport, new LinearLayout.LayoutParams(-1, 0, 1));
        TextView heading = text(getString(R.string.share_instruction), 24, Color.rgb(30, 39, 35)); heading.setPadding(0, dp(24), 0, dp(12)); sharePage.addView(heading);
        wifiBanner = text(getString(R.string.connect_wifi_first), 14, Color.rgb(61, 91, 77)); wifiBanner.setPadding(0, dp(8), 0, dp(8)); sharePage.addView(wifiBanner);
        details = new Button(this); details.setText(R.string.details); styleButton(details, false); sharePage.addView(details);
        ownAddress = text("", 16, Color.rgb(30, 39, 35)); ownAddress.setTextIsSelectable(true); ownAddress.setVisibility(View.GONE); sharePage.addView(ownAddress);
        details.setOnClickListener(v -> { detailsExpanded = !detailsExpanded; ownAddress.setVisibility(detailsExpanded ? View.VISIBLE : View.GONE); details.setText(detailsExpanded ? R.string.hide_details : R.string.details); });
        Button copyLink = new Button(this); copyLink.setText(R.string.copy_code); styleButton(copyLink, false); sharePage.addView(copyLink);
        copyLink.setOnClickListener(v -> {
            String link = ScreenShareService.invitation;
            if (link.isEmpty()) { shareStatus.setText(R.string.start_sharing_first); return; }
            android.content.ClipData clip = android.content.ClipData.newPlainText(getString(R.string.clip_code_label), link);
            if (android.os.Build.VERSION.SDK_INT >= 33) { android.os.PersistableBundle extras = new android.os.PersistableBundle(); extras.putBoolean(android.content.ClipDescription.EXTRA_IS_SENSITIVE, true); clip.getDescription().setExtras(extras); }
            getSystemService(android.content.ClipboardManager.class).setPrimaryClip(clip);
            shareStatus.setText(R.string.code_copied);
        });
        shareControl = new CheckBox(this); shareControl.setId(SHARE_CONTROL_ID); shareControl.setText(R.string.allow_phone_control); sharePage.addView(shareControl);
        shareControl.setOnCheckedChangeListener((v, enabled) -> {
            if (!enabled && ScreenShareService.sharing) ScreenShareService.revokeControl();
            if (enabled && !RemoteControlService.available()) {
                shareControl.setChecked(false);
                new android.app.AlertDialog.Builder(this).setTitle(R.string.phone_control_title).setMessage(R.string.phone_control_message).setPositiveButton(R.string.open_settings, (dialog, which) -> startActivity(new android.content.Intent(android.provider.Settings.ACTION_ACCESSIBILITY_SETTINGS))).setNegativeButton(R.string.cancel, null).show();
            }
        });
        Button keyboardSettings = new Button(this); keyboardSettings.setText(R.string.typing_options); styleButton(keyboardSettings, false); sharePage.addView(keyboardSettings);
        keyboardSettings.setOnClickListener(v -> new android.app.AlertDialog.Builder(this).setTitle(R.string.typing_options).setMessage(R.string.typing_options_message).setPositiveButton(R.string.enable, (dialog, which) -> startActivity(new android.content.Intent(android.provider.Settings.ACTION_INPUT_METHOD_SETTINGS))).setNeutralButton(R.string.select, (dialog, which) -> getSystemService(android.view.inputmethod.InputMethodManager.class).showInputMethodPicker()).setNegativeButton(R.string.cancel, null).show());
        share = new Button(this); share.setId(SHARE_ID); share.setText(R.string.share_this_screen); styleButton(share, true); sharePage.addView(share);
        shareStatus = text(ScreenShareService.summary.isEmpty() ? getString(R.string.not_sharing) : ScreenShareService.summary, 14, Color.rgb(61, 91, 77)); sharePage.addView(shareStatus);
        share.setOnClickListener(v -> {
            if (ScreenShareService.sharing) { stopService(new android.content.Intent(this, ScreenShareService.class)); return; }
            if (android.os.Build.VERSION.SDK_INT >= 33) {
                notificationPermissionDenied = checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED;
                boolean alreadyAsked = getPreferences(MODE_PRIVATE).getBoolean("notification_permission_requested", false);
                if (notificationPermissionDenied && !alreadyAsked) {
                    getPreferences(MODE_PRIVATE).edit().putBoolean("notification_permission_requested", true).apply();
                    requestPermissions(new String[]{Manifest.permission.POST_NOTIFICATIONS}, 201);
                    return;
                }
            }
            requestScreenCapture();
        });
        setContentView(layout); handler.post(refreshSharing);
        String incoming = getIntent().getDataString();
        if (android.content.Intent.ACTION_VIEW.equals(getIntent().getAction()) && incoming != null && incoming.startsWith("mightymonitor://") && incoming.length() <= 512) address.setText(incoming);

    }
    @Override protected void onNewIntent(android.content.Intent intent) { super.onNewIntent(intent); setIntent(intent); recreate(); }
    private android.graphics.drawable.GradientDrawable rounded(int color) {
        android.graphics.drawable.GradientDrawable shape = new android.graphics.drawable.GradientDrawable(); shape.setColor(color); shape.setCornerRadius(dp(8)); return shape;
    }
    private void styleButton(Button button, boolean primary) {
        button.setAllCaps(false); button.setTextSize(16); button.setElevation(0); button.setStateListAnimator(null); button.setMinHeight(dp(48)); button.setPadding(dp(14), dp(10), dp(14), dp(10));
        android.graphics.drawable.StateListDrawable background = new android.graphics.drawable.StateListDrawable();
        background.addState(new int[]{-android.R.attr.state_enabled}, rounded(Color.rgb(223, 231, 226)));
        background.addState(new int[]{android.R.attr.state_pressed}, rounded(Color.rgb(30, 91, 60)));
        background.addState(new int[]{}, rounded(primary ? Color.rgb(38, 115, 77) : Color.rgb(235, 241, 237)));
        button.setBackground(background); button.setTextColor(primary ? Color.WHITE : Color.rgb(30, 65, 43));
    }
    private void styleTab(Button button, boolean selected) {
        styleButton(button, false); button.setBackground(rounded(selected ? Color.rgb(217, 236, 223) : Color.rgb(247, 248, 247)));
        button.setTypeface(android.graphics.Typeface.create(selected ? "sans-serif-medium" : "sans-serif", android.graphics.Typeface.NORMAL));
        button.setSelected(selected);
    }
    private LinearLayout column() { LinearLayout layout = new LinearLayout(this); layout.setOrientation(LinearLayout.VERTICAL); return layout; }
    @Override protected void onActivityResult(int request, int result, android.content.Intent data) {
        super.onActivityResult(request, result, data);
        if (request == 200 && result == RESULT_OK && data != null) {
            android.content.Intent service = new android.content.Intent(this, ScreenShareService.class).putExtra("resultCode", result).putExtra("permission", data).putExtra("allowControl", shareControl.isChecked());
            startForegroundService(service);
        } else if (request == 200) shareStatus.setText(R.string.sharing_cancelled);
    }
    @Override public void onRequestPermissionsResult(int requestCode, String[] permissions, int[] grantResults) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults);
        if (requestCode == 201) {
            notificationPermissionDenied = grantResults.length == 0 || grantResults[0] != PackageManager.PERMISSION_GRANTED;
            requestScreenCapture();
        }
    }
    private void requestScreenCapture() {
        android.media.projection.MediaProjectionManager manager = getSystemService(android.media.projection.MediaProjectionManager.class);
        android.content.Intent permission = android.os.Build.VERSION.SDK_INT >= 34 ? manager.createScreenCaptureIntent(android.media.projection.MediaProjectionConfig.createConfigForDefaultDisplay()) : manager.createScreenCaptureIntent();
        startActivityForResult(permission, 200);
    }
    private TextView text(String value, int size, int color) {
        TextView view = new TextView(this); view.setText(value); view.setTextSize(size); view.setTextColor(color); return view;
    }
    private int dp(int value) { return Math.round(value * getResources().getDisplayMetrics().density); }
    private void startViewing() {
        if (!surfaceReady) { status.setText(R.string.display_not_ready); return; }
        if (!address.getText().toString().trim().startsWith("mightymonitor://") && !(trusted != null && trusted.isChecked())) { status.setText(R.string.paste_hint); return; }
        disconnect(getString(R.string.connecting));
        try {
            String endpoint = address.getText().toString().trim();
            handle = NativeBridge.connect(endpoint);
            if (handle == 0) throw new IllegalStateException("Could not start the network client");
            renderedFrames.set(0); connect.setText(R.string.disconnect);
            handler.post(pump);
        } catch (Exception | UnsatisfiedLinkError error) { android.util.Log.w("MightyMonitor", "Could not connect", error); disconnect(ErrorText.plain(error)); }
    }
    private static int androidKey(int code) {
        if (code >= 29 && code <= 54) return code - 29 + 4;
        if (code == 7) return 39; if (code >= 8 && code <= 16) return code - 8 + 30;
        switch (code) { case 66:return 40;case 111:return 41;case 67:return 42;case 61:return 43;case 62:return 44;case 19:return 82;case 20:return 81;case 21:return 80;case 22:return 79;case 113:case 114:return 224;case 59:case 60:return 225;case 57:case 58:return 226;case 117:case 118:return 227;default:return 0;}
    }
    private void consume(byte[] event) throws Exception {
        if (event.length == 0) throw new IllegalArgumentException("Empty network event");
        if (event[0] == 6) {
            if (event.length != 3) throw new IllegalArgumentException("Invalid control status");
            remoteControlActive = event[2] != 0; updatingControl = true; viewControl.setVisibility(event[1] != 0 ? View.VISIBLE : View.GONE); viewControl.setChecked(event[2] != 0); keyboard.setVisibility(event[2] != 0 ? View.VISIBLE : View.GONE); updatingControl = false; return;
        }
        if (event[0] == 5) { disconnect(ErrorText.plain(new String(event, 1, event.length - 1, StandardCharsets.UTF_8))); return; }
        if (event[0] == 1 || event[0] == 4) {
            if (event[0] == 4) status.setText(R.string.not_connected);
            else showViewerStatus(new String(event, 1, event.length - 1, StandardCharsets.UTF_8));
            if (event[0] == 4) releaseDecoder();
        } else if (event[0] == 2) {
            if (event.length != 13) throw new IllegalArgumentException("Invalid video configuration");
            ByteBuffer config = ByteBuffer.wrap(event).order(ByteOrder.LITTLE_ENDIAN);
            videoWidth = config.getInt(1); videoHeight = config.getInt(5);
            int fps = config.getInt(9);
            if (videoWidth <= 0 || videoHeight <= 0 || videoWidth > 8192 || videoHeight > 8192 || fps <= 0 || fps > 240) throw new IllegalArgumentException("Invalid video size");
            releaseDecoder();
            codec = MediaCodec.createDecoderByType(MediaFormat.MIMETYPE_VIDEO_AVC);
            MediaFormat format = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, videoWidth, videoHeight);
            format.setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, 16 * 1024 * 1024);
            codec.configure(format, video.getHolder().getSurface(), null, 0);
            codec.start();
            fitVideo();
        } else if (event[0] == 3) {
            if (event.length < 11 || codec == null) throw new IllegalArgumentException("Video arrived before configuration");
            pendingFrame = event;
        } else { throw new IllegalArgumentException("Unsupported network event"); }
    }
    private void showViewerStatus(String message) {
        if ("Connecting…".equals(message)) status.setText(R.string.connecting);
        else if ("Reconnecting…".equals(message)) status.setText(R.string.reconnecting);
        else if ("Connected — view only".equals(message)) status.setText(R.string.connected_view_only);
        else if ("Connected".equals(message)) status.setText(R.string.connected);
        else status.setText(ErrorText.plain(message));
    }
    private boolean queueFrame(byte[] event) {
        if (codec == null) return false;
        int input = codec.dequeueInputBuffer(0);
        if (input < 0) return false;
        ByteBuffer data = codec.getInputBuffer(input);
        if (data == null || data.capacity() < event.length - 10) throw new IllegalArgumentException("Video access unit exceeds decoder capacity");
        data.clear(); data.put(event, 10, event.length - 10);
        long timestamp = ByteBuffer.wrap(event).order(ByteOrder.LITTLE_ENDIAN).getLong(1);
        codec.queueInputBuffer(input, 0, event.length - 10, timestamp, event[9] != 0 ? MediaCodec.BUFFER_FLAG_KEY_FRAME : 0);
        return true;
    }
    private void drainOutput() {
        if (codec == null) return;
        for (int count = 0; count < 8; count++) {
            int output = codec.dequeueOutputBuffer(bufferInfo, 0);
            if (output >= 0) { codec.releaseOutputBuffer(output, true); renderedFrames.incrementAndGet(); emptyPreview.setVisibility(View.GONE); }
            else if (output != MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) break;
        }
    }
    private void fitVideo() {
        if (videoWidth == 0 || videoHeight == 0 || video.getParent() == null) return;
        View parent = (View) video.getParent();
        int w = parent.getWidth(), h = parent.getHeight();
        if (w == 0 || h == 0) return;
        float scale = Math.min((float) w / videoWidth, (float) h / videoHeight);
        int fittedWidth = Math.max(1, Math.round(videoWidth * scale));
        int fittedHeight = Math.max(1, Math.round(videoHeight * scale));
        if (video.getLayoutParams().width == fittedWidth && video.getLayoutParams().height == fittedHeight) return;
        android.widget.FrameLayout.LayoutParams params = new android.widget.FrameLayout.LayoutParams(fittedWidth, fittedHeight, Gravity.CENTER);
        video.setLayoutParams(params);
    }
    private void releaseDecoder() {
        pendingFrame = null;
        if (codec != null) { try { codec.stop(); } catch (IllegalStateException ignored) {} codec.release(); codec = null; }
    }
    private void disconnect(String message) {
        remoteControlActive = false;
        if (viewControl != null) {updatingControl = true; viewControl.setChecked(false); viewControl.setVisibility(View.GONE); keyboard.setVisibility(View.GONE); updatingControl = false;}
        handler.removeCallbacks(pump);
        long previous = handle; handle = 0;
        if (previous != 0) cleanup.execute(() -> NativeBridge.disconnect(previous));
        releaseDecoder();
        if (connect != null) connect.setText(R.string.connect);
        if (emptyPreview != null) emptyPreview.setVisibility(View.VISIBLE);
        if (status != null) status.setText(message);
    }
    public int getRenderedFrames() { return renderedFrames.get(); }
    @Override public void surfaceCreated(SurfaceHolder holder) { surfaceReady = true; connect.setEnabled(true); }
    @Override public void surfaceChanged(SurfaceHolder holder, int format, int width, int height) { fitVideo(); }
    @Override public void surfaceDestroyed(SurfaceHolder holder) { surfaceReady = false; connect.setEnabled(false); disconnect(getString(R.string.display_closed)); }
    @Override public void onPause() { disconnect(getString(R.string.not_connected)); super.onPause(); }
    @Override public void onDestroy() { disconnect(getString(R.string.not_connected)); handler.removeCallbacks(refreshSharing); cleanup.shutdown(); super.onDestroy(); }
}
