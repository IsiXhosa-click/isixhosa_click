import { LiveSearch, formatResult } from "/live_search.js";

let current_linked_word_id = 0;

function removeLinkedWord(button_id, search) {
    let button = document.getElementById(button_id);
    let cell = button.parentElement;
    let row = cell.parentElement;
    row.remove();
    search.stop();
}

function createLinkedWordSearch(preset_word) {
    let input = document.createElement("input");

    let popover_container = document.createElement("div");
    popover_container.className = "select_popover_container";
    popover_container.hidden = true;

    let popover = document.createElement("ol");
    popover_container.appendChild(popover);
    popover.className = "select_popover";

    function selectFocusOut() {
        setTimeout(function() {
            if (!(popover_container.contains(document.activeElement)) || input === document.activeElement) {
                popover_container.hidden = true;
            }
        }, 100);
    }

    function selectFocusIn() {
        popover_container.hidden = false;
    }

    input.placeholder = "Search for a linked word...";
    input.className = "word_select_search";
    input.type = "text";
    input.name = `linked_words[${current_linked_word_id}][other]`;
    input.autocomplete = "off";
    input.setAttribute("data-lpignore", "true");
    input.addEventListener("focus", selectFocusIn);
    input.addEventListener("blur", selectFocusOut);

    if (preset_word != null) {
        let last_choice = formatResult(preset_word);
        input.value = last_choice;
        input.setAttribute("data-last_search", preset_word.xhosa);
        input.setAttribute("data-last_choice", last_choice);
        input.setAttribute("data-selected_word_id", preset_word.id);
    }

    function createLinkedWordButton(word, word_id) {
        let button = document.createElement("button");
        button.type = "button";
        button.className = "select_list_option"
        button.addEventListener("blur", selectFocusOut);
        button.addEventListener("focus", selectFocusIn);
        button.addEventListener("click", function () {
            input.setAttribute("data-last_search", input.value);
            input.setAttribute("data-last_choice", word);
            input.setAttribute("data-selected_word_id", word_id);
            input.setAttribute("data-restore_choice", "false");
            input.value = word;
            button.blur();
        })
        return button;
    }

    function createLinkedWordContainer() {
        let item = document.createElement("li");
        item.className = "select_list_option"
        return item;
    }

    input.addEventListener("blur", function() {
        setTimeout(function() {
            if (popover_container.contains(document.activeElement) || input === document.activeElement) {
                return;
            }

            popover_container.hidden = true;

            if (input.getAttribute("data-restore_choice") !== "false") {
                input.setAttribute("data-last_search", input.value);
                input.value = input.getAttribute("data-last_choice");
            } else {
                input.setAttribute("data-restore_choice", "true");
            }
        }, 100);
    });

    input.addEventListener("focus", function () {
        popover_container.hidden = false;
        input.value = input.getAttribute("data-last_search");
    });

    return { input: input, popover: popover_container, search: new LiveSearch(input, popover, function() {}, createLinkedWordButton, createLinkedWordContainer) };
}

export function addLinkedWord(link_type, other, suggestion_id) {
    current_linked_word_id += 1;
    let table = document.getElementById("linked_words");
    let row = table.insertRow(table.rows.length - 1);

    let delete_cell = row.insertCell();
    let delete_button = document.createElement("button");
    delete_button.type = "button";
    delete_button.innerText = "Delete";
    delete_button.id = `linked_word-${current_linked_word_id}`;
    delete_cell.appendChild(delete_button);

    let type_cell = row.insertCell();
    let type_select = document.createElement("select");
    type_select.name = `linked_words[${current_linked_word_id}][link_type]`;

    const list = [
        { value: "", text: "Choose how the words are related" },
        { value: "1", text: "Singular or plural form" },
        { value: "2", text: "Synonym" },
        { value: "3", text: "Antonym" },
        { value: "4", text: "Related meaning" },
        { value: "5", text: "Confusable" }
    ];

    for (let i = 0; i < list.length; i++) {
        let option = document.createElement("option");
        option.innerText = list[i].text;
        option.value = list[i].value;

        if (i === link_type) {
            option.selected = true;
        }

        type_select.add(option);
    }

    type_cell.appendChild(type_select);

    if (suggestion_id != null) {
        let suggestion = document.createElement("select");
        suggestion.name = `linked_words[${current_linked_word_id}][suggestion_id]`;
        suggestion.hidden = true;
        
        let option = document.createElement("option");
        option.value = suggestion_id;
        suggestion.add(option);
        
        type_cell.appendChild(suggestion);
    }

    let linked_word = row.insertCell();
    linked_word.className = "word_select_container";
    let { input, popover, search } = createLinkedWordSearch(other);
    delete_button.addEventListener("click", function() { removeLinkedWord(this.id, search) });
    linked_word.appendChild(input);
    linked_word.appendChild(popover);
}

export function addLinkedWords(linked_words) {
    for (let linked_word of linked_words) {
        addLinkedWord(linked_word.link_type, linked_word.other, linked_word.suggestion_id)
    }

    if (linked_words.length === 0) {
        addLinkedWord()
    }
}
