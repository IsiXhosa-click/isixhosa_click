import { LiveSearch } from "/live_search.js";
import { addFormData } from "/submit/util.js";

let current_linked_word_id = 0;

function removeLinkedWord(translations, button_id, this_id) {
    let button = document.getElementById(button_id);
    let button_div = button.parentElement;
    let list_div = button_div.parentElement;
    let list_item = list_div.parentElement;
    list_item.remove();

    let delete_buttons = document.getElementsByClassName("delete_linked_word");
    if (delete_buttons.length === 0) {
        addLinkedWord(translations, this_id)
    }
}

function createLinkedWordSearch(translations, preset_word, preset_rendered, this_id) {
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

    input.placeholder = translations["linked-words.search"];
    input.className = "word_select_search";
    input.type = "text";
    input.name = `linked_words[${current_linked_word_id}][other]`;
    input.autocomplete = "off";
    input.setAttribute("aria-label", translations["linked-words.search"]);
    input.setAttribute("data-lpignore", "true");
    input.addEventListener("focus", selectFocusIn);
    input.addEventListener("blur", selectFocusOut);

    if (preset_word != null) {
        const div = document.createElement("div");
        div.innerHTML = preset_rendered;
        input.value = div.innerText;
        input.setAttribute("data-last_search", preset_word.xhosa);
        input.setAttribute("data-last_choice", div.innerText);
        input.setAttribute("data-selected_word_id", preset_word.id);
        input.setAttribute("data-selected_is_suggestion", preset_word.is_suggestion);
    }

    function createLinkedWordButton(word, word_id, is_suggestion) {
        let button = document.createElement("button");
        button.type = "button";
        button.className = "select_list_option";
        button.addEventListener("blur", selectFocusOut);
        button.addEventListener("focus", selectFocusIn);
        button.addEventListener("click", function () {
            input.setAttribute("data-last_search", input.value);
            input.setAttribute("data-last_choice", word);
            input.setAttribute("data-selected_word_id", word_id);
            input.setAttribute("data-selected_is_suggestion", is_suggestion);
            input.setAttribute("data-restore_choice", "false");
            input.value = word;
            button.blur();
        });
        return button;
    }

    function createLinkedWordContainer() {
        let item = document.createElement("li");
        item.className = "select_list_option";
        return [item, item];
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

    // noinspection EqualityComparisonWithCoercionJS -- this is done intentionally for string to number eq
    let search = new LiveSearch(
        input,
        popover,
        function() {},
        createLinkedWordButton,
        function() {},
        createLinkedWordContainer,
        r => r.id != this_id,
        true, // Include own suggestions
        translations
    );

    return { input: input, popover: popover_container, search: search };
}

export function addLinkedWord(translations, this_word_id, link_type, other, other_rendered, suggestion_id, existing_id) {
    current_linked_word_id += 1;
    let list = document.getElementById("linked_words");
    let item = document.createElement("li");
    list.insertBefore(item, document.getElementById("add_linked_word").parentElement);

    let div = document.createElement("div");
    item.appendChild(div);
    div.classList.add("row_list", "spaced_flex_list");

    let delete_button = document.createElement("button");
    delete_button.type = "button";

    let icon = document.getElementById("delete-button-template").content.cloneNode(true);
    delete_button.appendChild(icon);
    delete_button.setAttribute("aria-label", translations["delete"]);

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
        { value: "", text: `    ${translations["linked-words.choose"]}` },
        { value: "plural_or_singular", text: translations["linked-words.plurality"] },
        { value: "alternate_use", text: translations["linked-words.alternate"] },
        { value: "antonym", text: translations["linked-words.antonym"] },
        { value: "related", text: translations["linked-words.related"] },
        { value: "confusable", text: translations["linked-words.confusable"] },
    ];

    for (let type of types_list) {
        let option = document.createElement("option");
        option.innerText = type.text;
        option.value = type.value;

        if (type.value === link_type) {
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
    let { input, popover, search } = createLinkedWordSearch(translations, other, other_rendered, this_word_id);
    delete_button.addEventListener("click", function() { removeLinkedWord(translations, this.id, this_word_id) });
    linked_word.appendChild(input);
    linked_word.appendChild(popover);
    select_input_container.appendChild(linked_word);

    let delete_buttons = document.getElementsByClassName("delete_linked_word");
}

export function addLinkedWords(translations, linked_words, this_id) {
    for (let linked_word of linked_words) {
        addLinkedWord(
            translations,
            this_id,
            linked_word.link_type,
            linked_word.other,
            linked_word.other_rendered_plaintext,
            linked_word.suggestion_id,
            linked_word.existing_id
        )
    }

    if (linked_words.length === 0) {
        addLinkedWord(translations, this_id)
    }
}
