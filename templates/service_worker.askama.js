const OFFLINE_URL = "/offline";
const CACHE_NAME = "isixhosa_click_cache_v1";

// noinspection UnterminatedStatementJS
const SITE_FILES = {{ static_files|json }};

self.addEventListener("install", (event) => {
    console.log("Install!");
    event.waitUntil(
        (async () => {
            const cache = await caches.open(CACHE_NAME);
            await cache.add(new Request(OFFLINE_URL, { cache: "reload" }));
            await cache.addAll(SITE_FILES);
        })()
    );
    self.skipWaiting();
});

self.addEventListener('activate', function(event) {
    console.log("Activate!");
    event.waitUntil(
        caches.keys().then(function(cacheNames) {
            return Promise.all(
                cacheNames.filter(function(cacheName) {
                    cacheName.endsWith("js") || cacheName.endsWith("css") || cacheName.endsWith("webmanifest")
                }).map(function(cacheName) {
                    console.log("Deleting " + cacheName);
                    return caches.delete(cacheName);
                })
            );
        })
    );
});

self.addEventListener('fetch', function(event) {});

self.addEventListener("activate", (event) => {
    event.waitUntil(
        (async () => {
            if ("navigationPreload" in self.registration) {
                await self.registration.navigationPreload.enable();
            }
        })()
    );

    self.clients.claim();
});

self.addEventListener("fetch", (event) => {
    if (event.request.mode === "navigate") {
        event.respondWith(
            (async () => {
                try {
                    const preloadResponse = await event.preloadResponse;
                    if (preloadResponse) {
                        return preloadResponse;
                    }

                    return await fetch(event.request);
                } catch (error) {
                    console.log("Returning offline page as user seems to be offline", error);
                    let cache = await caches.open(CACHE_NAME);
                    return await cache.match(OFFLINE_URL);
                }
            })()
        );
    } else {
        event.respondWith(
            fetch(event.request).catch(async function() {
                let cache = await caches.open(CACHE_NAME);
                return await cache.match(event.request);
            })
        );
    }
});
