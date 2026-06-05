export class WebPlatformIoJs {
    constructor() {
        const openRequest = window.indexedDB.open("nullspace", 1);

        openRequest.onsuccess = (event) => {
            this.db = event.target.result;
        };

        openRequest.onupgradeneeded = (event) => {
            const db = event.target.result;
            const zoneStore = db.createObjectStore("zone", {
                keyPath: "filename",
            });

            zoneStore.createIndex("filename", "filename");
        };
    }

    save_zone_file(zone, filename, checksum, data) {
        return new Promise((resolve, reject) => {
            const transaction = this.db.transaction("zone", "readwrite")
            const zoneStore = transaction.objectStore("zone");

            transaction.oncomplete = () => { };

            const request = zoneStore.put({
                filename: filename,
                data: data,
                checksum: checksum
            });

            request.onerror = (event) => {
                reject();
            };

            request.onsuccess = (event) => {
                resolve();
            };
        });
    }

    load_zone_file(zone, filename) {
        return new Promise((resolve, reject) => {
            const zoneStore = this.db.transaction("zone").objectStore("zone");
            const request = zoneStore.get(filename);

            request.onerror = (event) => {
                reject();
            };

            request.onsuccess = (event) => {
                const result = request.result;

                if (result == undefined || result.data == undefined) {
                    reject();
                } else {
                    resolve(result.data);
                }
            };
        });
    }
}
