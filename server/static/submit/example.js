import { addFormData } from "/submit/util.js";

let current_example_id = 0;

function removeExample(translations, button_id) {
    let button = document.getElementById(button_id);
    let button_div = button.parentElement;
    let list_div = button_div.parentElement;
    let list_item = list_div.parentElement;
    list_item.remove();

    let delete_buttons = document.getElementsByClassName("delete_example");
    if (delete_buttons.length === 0) {
        addExample(translations)
    }
}

function textField(name, label_txt, val, english) {
    let div = document.createElement("div");
    div.className = "table_row_if_space";
    let label = document.createElement("label");
    let textarea = document.createElement("textarea");

    label.innerText = label_txt;
    textarea.name = name;
    textarea.autocomplete = "off";
    textarea.spellcheck = english;

    if (val != null) {
        textarea.value = val;
    }

    if (!english) {
        textarea.lang = "xh";
    }

    textarea.setAttribute("data-lpignore", "true");

    div.appendChild(label);
    div.appendChild(textarea);
    return div;
}

export function addExample(translations, english, xhosa, suggestion_id, existing_id) {
    current_example_id += 1;
    let list = document.getElementById("examples");
    let item = document.createElement("li");
    list.insertBefore(item, document.getElementById("add_example").parentElement);

    let div = document.createElement("div");
    item.appendChild(div);
    div.classList.add("spaced_flex_list", "row_list");

    let delete_button = document.createElement("button");
    delete_button.type = "button";

    let icon = document.getElementById("delete-button-template").content.cloneNode(true);
    delete_button.appendChild(icon);
    delete_button.setAttribute("aria-label", translations["delete"]);

    delete_button.addEventListener("click", function() { removeExample(translations, this.id) });
    delete_button.id = `example-${current_example_id}`;
    delete_button.classList.add("delete_example", "delete_button");
    let delete_div = document.createElement("div");
    delete_div.className = "delete_button_container";
    delete_div.appendChild(delete_button);
    div.appendChild(delete_div);

    if (suggestion_id != null) {
        div.appendChild(addFormData(`examples[${current_example_id}][suggestion_id]`, suggestion_id));
    }

    if (existing_id != null) {
        div.appendChild(addFormData(`examples[${current_example_id}][existing_id]`, existing_id));
    }

    let sentence = document.createElement("div");
    sentence.className = "row_or_column";
    div.appendChild(sentence);
    sentence.classList.add("table_if_space");

    sentence.appendChild(textField(`examples[${current_example_id}][english]`, `${translations["examples.source"]}:`, english, true));
    sentence.appendChild(textField(`examples[${current_example_id}][xhosa]`, `${translations["examples.target"]}:`, xhosa, false));

    let delete_buttons = document.getElementsByClassName("delete_example");
}

export function addExamples(translations, examples) {
    for (let example of examples) {
        addExample(translations, example.english, example.xhosa, example.suggestion_id, example.existing_id)
    }

    if (examples.length === 0) {
        addExample(translations)
    }
}
