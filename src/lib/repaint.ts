// Tauri's webview (WebKitGTK on Linux / WebView2 on Windows) occasionally
// fails to repaint after DOM mutations, leaving overlapping or blank regions
// until the user scrolls or resizes. This works around the compositor bug.
//
// Strategy:
//  1. CSS layer promotion (`transform: translateZ(0)` on #app / main) is the
//     primary fix — it puts the content on its own composited layer so the
//     webview repaints correctly without JS.
//  2. This JS is a fallback for structural DOM changes that still miss a
//     frame (e.g. Svelte `{#if}` swapping whole tab contents). It forces a
//     synchronous layout at most once per animation frame.
//
// Previous version observed `attributes: true` + `characterData: true` on
// `document.documentElement`, so every keystroke / class toggle triggered a
// forced reflow. Now scoped to structural `childList` changes on `#app` only.

export function installRepaintFix(): () => void {
  if (typeof window === "undefined" || typeof MutationObserver === "undefined") return () => {};
  if (typeof requestAnimationFrame === "undefined") return () => {};

  const target = document.getElementById("app") ?? document.documentElement;

  let scheduled = false;
  const observer = new MutationObserver(() => {
    if (scheduled) return;
    scheduled = true;
    requestAnimationFrame(() => {
      scheduled = false;
      // Reading offsetHeight forces a synchronous reflow, which prompts the
      // webview's compositor to repaint the affected regions.
      void target.offsetHeight;
    });
  });

  observer.observe(target, {
    childList: true,
    subtree: true,
  });

  return () => observer.disconnect();
}
