let current_example_id = 0;

function removeExample(button_id) {
    let button = document.getElementById(button_id);
    let cell = button.parentElement;
    let row = cell.parentElement;
    row.remove();
}

function textField(name, label_txt, val) {
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

    label.appendChild(input);
    return label;
}

export function addExample(english, xhosa) {
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

    let sentence = row.insertCell();
    sentence.className = "column_list";
    sentence.appendChild(textField(`examples[${current_example_id}][english]`, "English example:", english));
    sentence.appendChild(textField(`examples[${current_example_id}][xhosa]`, "Xhosa example:", xhosa));
}

export function addExamples(examples) {
    for (let example of examples) {
        console.log("Adding example");
        console.log(example);
        addExample(example.english, example.xhosa)
    }

    if (examples.length === 0) {
        addExample()
    }
}
