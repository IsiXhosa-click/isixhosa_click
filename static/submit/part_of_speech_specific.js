function setAllVisible(class_name, visible) {
    let optional_list = document.getElementsByClassName(class_name);

    for (let i = 0; i < optional_list.length; i++) {
        let option = optional_list.item(i);
        option.hidden = !visible;
    }
}

function setAllInvisible() {
    setAllVisible("noun_option", false);
    setAllVisible("verb_option", false);
    setAllVisible("conjunction_option", false);
}

export function partOfSpeechChange() {
    setAllInvisible();

    if (document.getElementById("verb_selected").selected) {
        setAllVisible("verb_option", true);
    } else if (document.getElementById("noun_selected").selected) {
        setAllVisible("noun_option", true);
    } else if (document.getElementById("conjunction_selected").selected) {
        setAllVisible("conjunction_option", true);
    }
}
