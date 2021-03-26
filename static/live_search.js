export class LiveSearch {
    constructor(search_container, results_container) {
        let input = document.createElement("input");
        input.setAttribute("type", "search");
        search_container.appendChild(input);
        input.focus();

        this.last_value = "";
        this.hits = results_container;
        this.ws = new WebSocket("ws://" + location.host + "/search");

        let search = this;

        this.ws.onopen = function() {
            search.ws.send("");
            search.ping_task = setInterval(function() { search.ws.send(""); }, 10000);

            search.refresh_task = setInterval(function() {
                if (search.last_value !== input.value) {
                    search.ws.send(input.value);
                    search.last_value = input.value;
                }

                if (input.value === "") {
                    search.hits.innerHTML = "";
                }
            }, 250);
        };

        this.ws.onmessage = function(event) {
            const data = JSON.parse(event.data);
            search.hits.innerHTML = "";

            if (data.hits.length === 0) {
                let p = document.createElement("p");
                let node = document.createTextNode("No results.");
                p.appendChild(node);

                search.hits.appendChild(p);
            } else {
                let list = document.createElement("ol");

                data.hits.forEach(function (hit) {
                    let result = hit.document;
                    let item = document.createElement("li");

                    let noun_class = "";
                    if (result.noun_class != null) {
                        noun_class = ` - Class ${result.noun_class}`;
                    }

                    const text = document.createTextNode(`${result.english} - ${result.xhosa} (${result.part_of_speech}${noun_class})`);
                    item.appendChild(text);
                    list.appendChild(item)
                });

                search.hits.appendChild(list);
            }
        };
    }

    stop() {
        clearInterval(this.refresh_task);
        clearInterval(this.ping_task);
        this.ws.close();
    }
}
