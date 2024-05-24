const OFFLINE_URL = "/offline";
const STATIC_FILES_CACHE = "isixhosa_click_site_files_[[ STATIC_LAST_CHANGED ]]";
const STATIC_BIN_FILES_CACHE = "isixhosa_click_site_bin_files_[[ STATIC_BIN_FILES_LAST_CHANGED ]]";

const SITE_FILES = [[ static_files|json|safe ]];
const BIN_FILES = [[ static_bin_files|json|safe ]];

self.addEventListener("install", (event) => {
    console.log("Install!");
    self.skipWaiting();

    event.waitUntil(
        (async () => {
            const site_files_cache = await caches.open(STATIC_FILES_CACHE);
            await site_files_cache.add(new Request(OFFLINE_URL, { cache: "reload" }));
            await site_files_cache.addAll(SITE_FILES.map((url) => new Request(url, { cache: 'reload' })));

            const bin_files_cache = await caches.open(STATIC_BIN_FILES_CACHE);
            await bin_files_cache.addAll(BIN_FILES.map((url) => new Request(url, { cache: 'reload' })));
        })()
    );
});

self.addEventListener("activate", function(event) {
    console.log("Activate!");
    event.waitUntil(
        caches.keys().then(function(cacheNames) {
            return Promise.all(
                cacheNames.filter(function(cacheName) {
                    return (cacheName !== STATIC_FILES_CACHE && cacheName !== STATIC_BIN_FILES_CACHE);
                }).map(function(cacheName) {
                    console.log("Deleting " + cacheName);
                    return caches.delete(cacheName);
                })
            );
        })
    );
});

self.addEventListener("activate", (event) => {
    event.waitUntil(
        (async () => {
            if ("navigationPreload" in self.registration) {
                await self.registration.navigationPreload.enable();
            }
        })()
    );

    console.log("I am a service worker with cache from unix epoch [[ STATIC_LAST_CHANGED ]]");
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
                    let cache = await caches.open(STATIC_FILES_CACHE);
                    return await cache.match(OFFLINE_URL);
                }
            })()
        );
    } else {
        event.respondWith(
            caches.match(event.request)
                .then(function(response) {
                    // Return cached response
                    if (response) {
                        return response;
                    } else {
                        // Fall back to fetch from internet
                        return fetch(event.request)
                            .catch(function(err) {
                                return caches.open(STATIC_FILES_CACHE)
                                    .then(function(cache) {
                                        console.error(`Error fetching: ${err}`);
                                        return cache.match(OFFLINE_URL);
                                    });
                            });
                    }
                })
        );
    }
});
