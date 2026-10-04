package org.octosense.rinx;

import android.content.Intent;
import android.content.ComponentCallbacks;
import android.content.res.Configuration;
import android.util.Log;

import java.lang.reflect.Field;

import dev.makepad.android.MakepadActivity;
import dev.makepad.android.MakepadNative;

/**
 * Rinx's hook into Makepad's Android activity (cargo-makepad loads
 * {@code <package>.MakepadAppExtension} by name in onCreate).
 *
 * <p>Back at the root of Rinx's navigation backgrounds the app, as Android
 * does for a root activity; Makepad's view swallows the key otherwise.
 *
 * <p>Makepad's activity asks for location in every app's first onResume, to
 * feed OctoSense's navigation card. Rinx only needs location when the person
 * shares it, and robius-location asks for it then. Asking at launch put the
 * permission dialog over Rinx's first frame on every fresh install, so the
 * extension marks that request as already made. This hook can go once
 * cargo-makepad lets an app opt out of the launch-time location request.
 */
public final class MakepadAppExtension implements MakepadActivity.ApplicationExtension {
    private static final String TAG = "Rinx";

    private final MakepadActivity mActivity;
    private final ComponentCallbacks appearanceCallbacks = new ComponentCallbacks() {
        @Override public void onConfigurationChanged(Configuration config) { publishAppearance(); }
        @Override public void onLowMemory() {}
    };
    private void publishAppearance() {
        int mode = mActivity.getResources().getConfiguration().uiMode & Configuration.UI_MODE_NIGHT_MASK;
        MakepadNative.onAndroidIntegrationEvent("rinx.appearance", mode == Configuration.UI_MODE_NIGHT_YES ? "dark" : "light");
    }

    public MakepadAppExtension(MakepadActivity activity) {
        mActivity = activity;
        activity.registerComponentCallbacks(appearanceCallbacks);
        try {
            Field requested = MakepadActivity.class.getDeclaredField("mLocationPermissionRequested");
            requested.setAccessible(true);
            requested.setBoolean(activity, true);
        } catch (ReflectiveOperationException | RuntimeException e) {
            Log.w(TAG, "Could not defer Makepad's launch-time location request", e);
        }
    }

    /** Rinx's Rust side sends "rinx.back" for a Back nothing in Rinx took. */
    @Override public void command(String channel, String payload) {
        if ("rinx.back".equals(channel)) {
            mActivity.moveTaskToBack(true);
        }
        if ("rinx.appearance".equals(channel)) { publishAppearance(); }
    }
    @Override public void onResume() { publishAppearance(); }
    @Override public void onPause() {}
    @Override public void onIntent(Intent intent) {}
    @Override public void onDestroy() { mActivity.unregisterComponentCallbacks(appearanceCallbacks); }
}
