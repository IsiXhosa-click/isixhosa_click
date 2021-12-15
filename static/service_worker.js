const OFFLINE_URL = "/offline";
const CACHE_NAME = "isixhosa_click_cache_v1";

const SITE_FILES = [
    '/style.css?v=11',
    '/icons/icon-192.png',
    '/icons/icon-32.png',
    '/icons/icon-16.png',
    '/icons/favicon.ico',
    'manifest.webmanifest?v=2',
];

const EXTERNAL_FILES = [
    "https://fonts.googleapis.com/css?family=Open+Sans:400,600,700,800&display=swap",
    "https://fonts.googleapis.com/css?family=Roboto&display=swap",
    "https://fonts.googleapis.com/icon?family=Material+Icons",
];

self.addEventListener("install", (event) => {
    event.waitUntil(
        (async () => {
            const cache = await caches.open(CACHE_NAME);
            await cache.add(new Request(OFFLINE_URL, { cache: "reload" }));
            await cache.addAll(EXTERNAL_FILES);
            await cache.addAll(SITE_FILES);
        })()
    );
    self.skipWaiting();
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
