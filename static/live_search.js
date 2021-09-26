let ws;
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
        skip_word_id,
        skip_is_suggestion,
        include_own_suggestions
    ) {
        this.input = input;
        this.last_value = "";
        this.hits = results_container;
        this.create_container = create_container;
        this.create_item = create_item;
        this.post_create_item = post_create_item;
        this.create_item_container = create_item_container;
        this.skip_word_id = skip_word_id;
        this.skip_is_suggestion = skip_is_suggestion;

        this.id = next_id;
        next_id++;

        searchers[this.id] = this;

        if (ws == null) {
            ws = new WebSocket(
                "wss://" + location.host + `/search?include_own_suggestions=${include_own_suggestions}`
            );

            ws.onopen = function() {
                ws.send("");
                setInterval(function() { ws.send(""); }, 10000);
                setInterval(function() {
                    for (let searcher of Object.values(searchers)) {
                        searcher.refresh();
                    }
                }, 250);
            };

            ws.onerror = console.error;

            ws.onmessage = function(event) {
                let reply = JSON.parse(event.data);
                let searcher = searchers[reply.state];

                if (searcher != null) {
                    searcher.processResults(reply.results);
                }
            }
        }
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

        // noinspection EqualityComparisonWithCoercionJS -- this is done intentionally for string to number eq
        results = results.filter(result => !(result.id == searcher.skip_word_id && result.is_suggestion == searcher.skip_is_suggestion));

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

                let [item_container_parent, item_container_inner] = searcher.create_item_container(result.id);
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

const NOUN_CLASS_PAIRS = {
    1: ["um", "aba"],
    2: ["um", "aba"],

    3: ["u", "oo"],
    4: ["u", "oo"],

    5: ["um", "imi"],
    6: ["um", "imi"],

    7: ["ili", "ama"],
    8: ["ili", "ama"],

    9: ["isi", "izi"],
    10: ["isi", "izi"],

    11: ["in", "izin"],
    12: ["in", "izin"],

    13: ["ulu"],
    14: ["ubu"],
    15: ["uku"]
};

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
            let class_pair = NOUN_CLASS_PAIRS[result.noun_class];
            let strong = document.createElement("strong");
            strong.className = "noun_class_prefix";

            if (result.noun_class === class_pair[1]) {
                strong.innerText = class_pair[1]
                elt.innerText += ` - class ${class_pair[0]}/`
                elt.appendChild(strong);
            } else {
                strong.innerText = class_pair[0]
                elt.innerText += ` - class `
                elt.appendChild(strong);

                if (class_pair[1] != null) {
                    elt.innerHTML += `/${class_pair[1]}`;
                }
            }
        }

        elt.innerHTML += ")";
    } else {
        let noun_class = "";
        if (result.noun_class != null) {
            let class_pair = NOUN_CLASS_PAIRS[result.noun_class];

            if (class_pair[1] != null) {
                noun_class = ` - class ${class_pair[0]}/${class_pair[1]}`;
            } else {
                noun_class = ` - class ${class_pair[0]}`;
            }
        }

        return text + `${noun_class})`;
    }
}
