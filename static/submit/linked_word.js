import { LiveSearch, formatResult } from "/live_search.js";
import { addFormData } from "/submit/util.js";

let current_linked_word_id = 0;

function removeLinkedWord(button_id, search) {
    let button = document.getElementById(button_id);
    let button_div = button.parentElement;
    let list_div = button_div.parentElement;
    let list_item = list_div.parentElement;
    list_item.remove();
    search.stop();

    let delete_buttons = document.getElementsByClassName("delete_linked_word");
    if (delete_buttons.length === 1) {
        delete_buttons.item(0).disabled = true;
    }
}

// TODO filter out own word if submitted already
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

export function addLinkedWord(link_type, other, suggestion_id, existing_id) {
    current_linked_word_id += 1;
    let list = document.getElementById("linked_words");
    let item = document.createElement("li");
    list.insertBefore(item, document.getElementById("add_linked_word").parentElement);

    let div = document.createElement("div");
    item.appendChild(div);
    div.classList.add("row_list", "spaced_flex_list");

    let delete_button = document.createElement("button");
    delete_button.type = "button";

    let icon = document.createElement("span");
    icon.className = "material-icons";
    icon.innerText = "delete";
    delete_button.appendChild(icon);
    delete_button.setAttribute("aria-label", "delete");

    delete_button.id = `linked_word-${current_linked_word_id}`;
    delete_button.classList.add("delete_linked_word", "delete_button");
    let delete_div = document.createElement("div");
    delete_div.className = "delete_button_container";
    delete_div.appendChild(delete_button);
    div.appendChild(delete_div);

    let select_input_container = document.createElement("div");
    select_input_container.classList.add("column_list", "spaced_flex_list");
    div.appendChild(select_input_container);

    let type_select = document.createElement("select");
    type_select.className = "type_select";
    type_select.name = `linked_words[${current_linked_word_id}][link_type]`;

    const types_list = [
        { value: "", text: "Choose how the words are related" },
        { value: "1", text: "Singular or plural form" },
        { value: "2", text: "Synonym" },
        { value: "3", text: "Antonym" },
        { value: "4", text: "Related meaning" },
        { value: "5", text: "Confusable" }
    ];

    for (let i = 0; i < types_list.length; i++) {
        let option = document.createElement("option");
        option.innerText = types_list[i].text;
        option.value = types_list[i].value;

        if (i === link_type) {
            option.selected = true;
        }

        type_select.add(option);
    }

    select_input_container.appendChild(type_select);

    if (suggestion_id != null) {
        div.appendChild(addFormData(`linked_words[${current_linked_word_id}][suggestion_id]`, suggestion_id));
    }

    if (existing_id != null) {
        div.appendChild(addFormData(`linked_words[${current_linked_word_id}][existing_id]`, existing_id));
    }

    let linked_word = document.createElement("div");
    linked_word.className = "word_select_container";
    let { input, popover, search } = createLinkedWordSearch(other);
    delete_button.addEventListener("click", function() { removeLinkedWord(this.id, search) });
    linked_word.appendChild(input);
    linked_word.appendChild(popover);
    select_input_container.appendChild(linked_word);

    let delete_buttons = document.getElementsByClassName("delete_linked_word");
    if (delete_buttons.length > 1) {
        for (let button of delete_buttons) {
            button.disabled = false;
        }
    } else {
        delete_buttons.item(0).disabled = true;
    }
}

export function addLinkedWords(linked_words) {
    for (let linked_word of linked_words) {
        addLinkedWord(linked_word.link_type, linked_word.other, linked_word.suggestion_id, linked_word.existing_id)
    }

    if (linked_words.length === 0) {
        addLinkedWord()
    }
}
