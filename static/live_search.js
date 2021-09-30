let ws;
let reopen_last_tried = 0;
let next_id = 1;
let searchers = {};

export class LiveSearch {
    constructor(
        input,
        results_container,
        create_container,
        create_item,
        post_create_item,
        create_item_container,
        filter_fn,
        include_own_suggestions
    ) {
        this.input = input;
        this.last_value = "";
        this.hits = results_container;
        this.create_container = create_container;
        this.create_item = create_item;
        this.post_create_item = post_create_item;
        this.create_item_container = create_item_container;
        this.filter_fn = filter_fn;

        this.id = next_id;
        next_id++;

        searchers[this.id] = this;

        async function maybeOpenWs() {
            if (ws == null && (Date.now() - reopen_last_tried) > 1000) {
                reopen_last_tried = Date.now();

                ws = new WebSocket(
                    "wss://" + location.host + `/search?include_own_suggestions=${include_own_suggestions}`
                );

                ws.onopen = function () { ws.send(""); };
                ws.onerror = function() { ws = null; };
                ws.onclose = function() { ws = null; };

                ws.onmessage = function (event) {
                    let reply = JSON.parse(event.data);
                    let searcher = searchers[reply.state];

                    if (searcher != null) {
                        searcher.processResults(reply.results);
                    }
                }
            }
        }

        maybeOpenWs();

        setInterval(function () {
            if (ws != null && ws.readyState === WebSocket.OPEN) {
                ws.send("");
            }
        }, 10000);

        setInterval(function () {
            maybeOpenWs();

            if (ws != null && ws.readyState === WebSocket.OPEN) {
                for (let searcher of Object.values(searchers)) {
                    searcher.refresh();
                }
            }
        }, 250);
    }

    refresh() {
        if (this.input === document.activeElement && this.last_value !== this.input.value) {
            ws.send(JSON.stringify({ search: this.input.value, state: this.id.toString() }));
            this.last_value = this.input.value;
        }

        if (this.input.value === "") {
            this.hits.innerHTML = "";
            this.input.classList.remove("has_results");
        }
    }

    processResults(results) {
        let searcher = this;
        searcher.hits.innerHTML = "";

        results = results.filter(searcher.filter_fn);

        if (results.length === 0) {
            let p = document.createElement("p");
            p.className = "no_results";
            let node = document.createTextNode("No results.");
            p.appendChild(node);
            searcher.input.classList.remove("has_results");

            searcher.hits.appendChild(p);
        } else {
            let container = searcher.create_container();

            results.forEach(function (result) {
                let item = searcher.create_item(formatResult(result), result.id, result.is_suggestion);
                formatResult(result, item);

                let [item_container_parent, item_container_inner] = searcher.create_item_container(result.id, result.is_suggestion);
                let append = item;

                if (item_container_parent != null) {
                    item_container_inner.appendChild(item);
                    append = item_container_parent;
                }

                if (container != null) {
                    container.appendChild(append);
                } else {
                    searcher.hits.appendChild(append);
                }

                searcher.post_create_item(item);
            });

            searcher.input.classList.add("has_results");

            if (container != null) {
                searcher.hits.appendChild(container);
            }
        }
    }
}

export function formatResult(result, elt) {
    let plural = "";
    if (result.is_plural) {
        plural = "plural ";
    }

    let inchoative = "";
    if (result.is_inchoative) {
        inchoative = "inchoative ";
    }

    let transitive = "";
    if (result.transitivity != null && result.transitivity !== "") {
        transitive = result.transitivity + " ";
    }

    let part_of_speech = result.part_of_speech;
    if (part_of_speech === "adjective") {
        part_of_speech = "adjective (isiphawuli)";
    } else if (part_of_speech === "relative") {
        part_of_speech = "relative";
    }

    let text = `${result.english} - ${result.xhosa} (${inchoative}${transitive}${part_of_speech}`;

    if (elt != null) {
        elt.innerText = text;

        if (result.noun_class != null) {
            let strong = document.createElement("strong");
            strong.className = "noun_class_prefix";

            if (!result.noun_class.selected_singular) {
                strong.innerText = result.noun_class.plural;
                elt.innerText += ` - class ${result.noun_class.singular}/`
                elt.appendChild(strong);
            } else {
                strong.innerText = result.noun_class.singular
                elt.innerText += ` - class `
                elt.appendChild(strong);

                if (result.noun_class.plural != null) {
                    elt.innerHTML += `/${result.noun_class.plural}`;
                }
            }
        }

        elt.innerHTML += ")";
    } else {
        let noun_class = "";
        if (result.noun_class != null) {
            if (result.noun_class.plural != null) {
                noun_class = ` - class ${result.noun_class.singular}/${result.noun_class.plural}`;
            } else {
                noun_class = ` - class ${result.noun_class.singular}`;
            }
        }

        return text + `${noun_class})`;
    }
}
