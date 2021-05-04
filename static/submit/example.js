let current_example_id = 0;

function removeExample(button_id) {
    let button = document.getElementById(button_id);
    let cell = button.parentElement;
    let row = cell.parentElement;
    row.remove();
}

function textField(name, label_txt, val) {
    let div = document.createElement("div");
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

export function addExample(english, xhosa, suggestion_id) {
    current_example_id += 1;
    let table = document.getElementById("examples");
    let row = table.insertRow(table.rows.length - 1);

    let delete_cell = row.insertCell();
    let delete_button = document.createElement("button");
    delete_button.type = "button";
    delete_button.innerText = "Delete";
    delete_button.addEventListener("click", function() { removeExample(this.id) });
    delete_button.id = `example-${current_example_id}`;
    delete_cell.appendChild(delete_button);

    if (suggestion_id != null) {
        let suggestion = document.createElement("select");
        suggestion.name = `examples[${current_example_id}][suggestion_id]`;
        suggestion.hidden = true;

        let option = document.createElement("option");
        option.value = suggestion_id;
        suggestion.add(option);

        delete_cell.appendChild(suggestion);
    }

    let sentence = row.insertCell();
    sentence.classList.add("table", "column_list");
    sentence.appendChild(textField(`examples[${current_example_id}][english]`, "English example:", english));
    sentence.appendChild(textField(`examples[${current_example_id}][xhosa]`, "Xhosa example:", xhosa));
}

export function addExamples(examples) {
    for (let example of examples) {
        console.log("Adding example");
        console.log(example);
        addExample(example.english, example.xhosa, example.suggestion_id)
    }

    if (examples.length === 0) {
        addExample()
    }
}
