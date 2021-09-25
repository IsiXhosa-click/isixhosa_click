function setAllEnabled(class_name, enabled) {
    for (let elt of document.getElementsByClassName(class_name)) {
        elt.hidden = !enabled;
    }

    for (let elt of document.querySelectorAll(`.${class_name} .required_if_enabled`)) {
        elt.required = enabled;
    }
}

function setAllInvisible() {
    setAllEnabled("noun_option", false);
    setAllEnabled("verb_option", false);
    setAllEnabled("conjunction_option", false);
}

export function partOfSpeechChange() {
    setAllInvisible();

    if (document.getElementById("verb_selected").selected) {
        setAllEnabled("verb_option", true);
    } else if (document.getElementById("noun_selected").selected) {
        setAllEnabled("noun_option", true);
    } else if (document.getElementById("conjunction_selected").selected) {
        setAllEnabled("conjunction_option", true);
    }
}
