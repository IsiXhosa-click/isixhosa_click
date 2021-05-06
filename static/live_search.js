export class LiveSearch {
    constructor(
        input,
        results_container,
        create_container,
        create_item,
        create_item_container
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
                    input.classList.remove("has_results");
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
                input.classList.remove("has_results");

                search.hits.appendChild(p);
            } else {
                let container = search.create_container();

                data.hits.forEach(function (hit) {
                    let result = hit.document;
                    let item = search.create_item(formatResult(result), result.id);
                    formatResultRich(result, item);

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

                input.classList.add("has_results");

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

const NOUN_CLASSES = {
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

function formatResultRich(result, elt) {
    let plural = "";
    if (result.is_plural) {
        plural = "plural ";
    }

    elt.innerText = `${result.english} - ${result.xhosa} (${plural}${result.part_of_speech}`;

    if (result.noun_class != null) {
        let class_pair = NOUN_CLASSES[result.noun_class];
        let strong = document.createElement("strong");
        strong.className = "noun_class_prefix";

        if (result.is_plural) {
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
