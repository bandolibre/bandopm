const CACHE_NAME = "bandopm-pwa-v5-31";
const PRECACHE_ASSETS = [
  "./",
  "./index.html",
  "./app.js",
  "./worklet-processor.js",
  "./synth_wasm.wasm",
  "./manifest.webmanifest",
  "./icon.svg",
  "./icon-192.png",
  "./icon-512.png",
  "./icon-maskable-512.png"
];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => cache.addAll(PRECACHE_ASSETS))
  );
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches.keys().then((keys) =>
      Promise.all(
        keys
          .filter((key) => key !== CACHE_NAME)
          .map((oldKey) => caches.delete(oldKey))
      )
    )
  );
  self.clients.claim();
});

self.addEventListener("fetch", (event) => {
  if (event.request.method !== "GET") return;
  const reqUrl = new URL(event.request.url);
  if (reqUrl.origin !== self.location.origin) return;

  // Network-first with offline cache fallback so WASM/JS updates appear immediately
  // when online, while remaining 100% playable offline when installed as a PWA.
  event.respondWith(
    fetch(event.request)
      .then((networkResp) => {
        if (networkResp && networkResp.status === 200) {
          const clone = networkResp.clone();
          caches.open(CACHE_NAME).then((cache) => cache.put(event.request, clone));
        }
        return networkResp;
      })
      .catch(() =>
        caches.match(event.request, { ignoreSearch: true }).then((cachedResp) => {
          if (cachedResp) return cachedResp;
          if (event.request.mode === "navigate") {
            return caches.match("./index.html", { ignoreSearch: true });
          }
          return Response.error();
        })
      )
  );
});
