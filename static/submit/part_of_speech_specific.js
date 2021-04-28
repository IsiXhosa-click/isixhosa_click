function setAllVisible(class_name, visible) {
    let optional_list = document.getElementsByClassName(class_name);

    for (let i = 0; i < optional_list.length; i++) {
        let option = optional_list.item(i);
        option.hidden = !visible;
    }
}

export function partOfSpeechChange() {
    let part_of_speech = document.getElementById("part_of_speech").value;

    if (part_of_speech === "1") { // Verb
        setAllVisible("verb_option", true);
        setAllVisible("noun_option", false);
    } else if (part_of_speech === "2") { // Noun
        setAllVisible("noun_option", true);
        setAllVisible("verb_option", false);
    } else {
        setAllVisible("noun_option", false);
        setAllVisible("verb_option", false);
    }
}
