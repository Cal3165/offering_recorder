package com.caleb.offeringrecorder.slint;

import android.app.NativeActivity;
import android.content.Intent;
import android.net.Uri;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.concurrent.CountDownLatch;

public final class RecorderActivity extends NativeActivity {
    private static final int PICK_DOCUMENT = 4711;
    private static final int MAX_CSV = 5 * 1024 * 1024;
    private Selection pending;

    private static final class Selection {
        final CountDownLatch finished = new CountDownLatch(1);
        Uri uri;
        boolean failed;
    }

    // Invoked on the Rust database worker. Only the picker runs on the UI thread.
    public int pickDocument(String path, String name, String mime, boolean export) {
        Selection selection = new Selection();
        runOnUiThread(() -> {
            if (isFinishing() || isDestroyed() || pending != null) {
                selection.failed = true;
                selection.finished.countDown();
                return;
            }
            pending = selection;
            Intent intent = new Intent(export ? Intent.ACTION_CREATE_DOCUMENT : Intent.ACTION_OPEN_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            intent.setType(mime);
            if (export) {
                intent.putExtra(Intent.EXTRA_TITLE, name);
            }
            try {
                startActivityForResult(intent, PICK_DOCUMENT);
            } catch (RuntimeException unavailable) {
                complete(null, true);
            }
        });
        try {
            selection.finished.await();
            if (selection.failed) return -1;
            if (selection.uri == null) return 0;
            if (export) {
                try (InputStream input = new FileInputStream(path);
                     OutputStream output = getContentResolver().openOutputStream(selection.uri, "wt")) {
                    copy(input, output, Long.MAX_VALUE);
                }
            } else {
                try (InputStream input = getContentResolver().openInputStream(selection.uri);
                     OutputStream output = new FileOutputStream(path)) {
                    copy(input, output, MAX_CSV);
                }
            }
            return 1;
        } catch (LimitExceeded oversized) {
            return -2;
        } catch (InterruptedException interrupted) {
            Thread.currentThread().interrupt();
            return -1;
        } catch (IOException | RuntimeException inaccessible) {
            return -1;
        }
    }

    private static final class LimitExceeded extends IOException {}

    private static void copy(InputStream input, OutputStream output, long limit) throws IOException {
        if (input == null || output == null) throw new IOException("Document unavailable");
        byte[] buffer = new byte[16384];
        long total = 0;
        for (int read; (read = input.read(buffer)) != -1;) {
            total += read;
            if (total > limit) throw new LimitExceeded();
            output.write(buffer, 0, read);
        }
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode == PICK_DOCUMENT) {
            complete(resultCode == RESULT_OK && data != null ? data.getData() : null, false);
        }
    }

    @Override
    protected void onDestroy() {
        complete(null, false);
        super.onDestroy();
    }

    private void complete(Uri uri, boolean failed) {
        Selection selection = pending;
        pending = null;
        if (selection != null) {
            selection.uri = uri;
            selection.failed = failed;
            selection.finished.countDown();
        }
    }
}
