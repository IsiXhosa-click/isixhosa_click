import { addFormData } from "/submit/util.js";

let current_example_id = 0;

function removeExample(button_id) {
    let button = document.getElementById(button_id);
    let button_div = button.parentElement;
    let list_div = button_div.parentElement;
    let list_item = list_div.parentElement;
    list_item.remove();

    let delete_buttons = document.getElementsByClassName("delete_example");
    if (delete_buttons.length === 1) {
        delete_buttons.item(0).disabled = true;
    }
}

function textField(name, label_txt, val) {
    let div = document.createElement("div");
    div.className = "table_row_if_space";
    let label = document.createElement("label");
    let input = document.createElement("input");

    label.innerText = label_txt;
    input.type = "text";
    input.name = name;
    input.autocomplete = "off";

    if (val != null) {
        input.value = val;
    }

    input.setAttribute("data-lpignore", "true");

    div.appendChild(label);
    div.appendChild(input);
    return div;
}

export function addExample(english, xhosa, suggestion_id, existing_id) {
    current_example_id += 1;
    let list = document.getElementById("examples");
    let item = document.createElement("li");
    list.insertBefore(item, document.getElementById("add_example").parentElement);

    let div = document.createElement("div");
    item.appendChild(div);
    div.classList.add("spaced_flex_list", "row_list");

    let delete_button = document.createElement("button");
    delete_button.type = "button";

    let icon = document.createElement("span");
    icon.className = "material-icons";
    icon.innerText = "delete"
    delete_button.appendChild(icon);
    delete_button.setAttribute("aria-label", "delete");

    delete_button.addEventListener("click", function() { removeExample(this.id) });
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

    sentence.appendChild(textField(`examples[${current_example_id}][english]`, "English example:", english));
    sentence.appendChild(textField(`examples[${current_example_id}][xhosa]`, "Xhosa example:", xhosa));

    let delete_buttons = document.getElementsByClassName("delete_example");
    if (delete_buttons.length > 1) {
        for (let button of delete_buttons) {
            button.disabled = false;
        }
    } else {
        delete_buttons.item(0).disabled = true;
    }
}

export function addExamples(examples) {
    for (let example of examples) {
        addExample(example.english, example.xhosa, example.suggestion_id, example.existing_id)
    }

    if (examples.length === 0) {
        addExample()
    }
}
