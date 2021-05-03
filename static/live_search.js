export class LiveSearch {
    constructor(
        input,
        results_container,
        create_container = function() { return document.createElement("ol"); },
        create_item = function() { return document.createElement("li"); },
        create_item_container = function() { return null; }
    ) {
        this.last_value = "";
        this.hits = results_container;
        this.ws = new WebSocket("ws://" + location.host + "/search");
        this.create_container = create_container;
        this.create_item = create_item;
        this.create_item_container = create_item_container;

        let search = this;

        this.ws.onopen = function() {
            search.ws.send("");
            search.ping_task = setInterval(function() { search.ws.send(""); }, 10000);

            search.refresh_task = setInterval(function() {
                if (input === document.activeElement && search.last_value !== input.value) {
                    search.ws.send(input.value);
                    search.last_value = input.value;
                }

                if (input.value === "") {
                    search.hits.innerHTML = "";
                }
            }, 250);
        };

        this.ws.onerror = console.error;

        this.ws.onmessage = function(event) {
            const data = JSON.parse(event.data);
            search.hits.innerHTML = "";

            if (data.hits.length === 0) {
                let p = document.createElement("p");
                let node = document.createTextNode("No results.");
                p.appendChild(node);

                search.hits.appendChild(p);
            } else {
                let container = search.create_container();

                data.hits.forEach(function (hit) {
                    let result = hit.document;
                    let str = formatResult(result);
                    let text = document.createTextNode(str);
                    let item = search.create_item(str, result.id);
                    item.appendChild(text);

                    let item_container = search.create_item_container();
                    let append = item;

                    if (item_container != null) {
                        item_container.appendChild(item);
                        append = item_container;
                    }

                    if (container != null) {
                        container.appendChild(append);
                    } else {
                        search.hits.appendChild(append);
                    }
                });

                if (container != null) {
                    search.hits.appendChild(container);
                }
            }
        };
    }

    stop() {
        clearInterval(this.refresh_task);
        clearInterval(this.ping_task);
        this.ws.close();
    }
}

export function formatResult(result) {
    let noun_class = "";
    if (result.noun_class != null) {
        noun_class = ` - class ${result.noun_class}`;
    }

    let plural = "";
    if (result.is_plural) {
        plural = "plural ";
    }

    return `${result.english} - ${result.xhosa} (${plural}${result.part_of_speech}${noun_class})`;
}
