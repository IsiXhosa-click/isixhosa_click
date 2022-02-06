use serde::Deserialize;
use std::fmt::{self, Display, Formatter, Write};
use std::iter;
use ascii::{AsciiChar, AsciiString, IntoAsciiString};
use gloo::events::EventListener;
use rand::prelude::*;
use tinyvec::ArrayVec;
use wasm_bindgen::JsCast;
use yew::{Callback, Component, Context, function_component, Html, html, Properties};

const CSV: &[u8] = include_bytes!("../words.csv");
const WORD_LENGTH: usize = 5;
const GUESSES: usize = 6;

#[derive(Clone, PartialEq)]
struct Key {
    key: AsciiChar,
    state: Option<CharGuessResult>,
}

impl Key {
    fn css(&self) -> &'static str {
        self.state.map(|s| s.into_css()).unwrap_or_default()
    }
}

impl From<AsciiChar> for Key {
    fn from(key: AsciiChar) -> Self {
        Key {
            key,
            state: None,
        }
    }
}

#[derive(Properties, PartialEq)]
struct KeyboardProps {
    keys: Vec<Vec<Key>>,
    on_click: Callback<AsciiChar>,
}

#[function_component(Keyboard)]
fn keyboard(props: &KeyboardProps) -> Html {
    props.keys.iter().map(|row| {
        let row = row.iter().map(|key| {
            let on_click = {
                let on_click = props.on_click.clone();
                let key = key.key.clone();
                Callback::from(move |_| {
                    on_click.emit(key.clone());
                })
            };


            if key.key.as_char() == '|' {
                html! {
                    <div class="spacer_key"></div>
                }
            } else {
                let (label, class) = match key.key.as_char() {
                    '\n' => ('↵', "key special_key"),
                    '\x08' => ('⌫', "key special_key"),
                    c => (c, "key"),
                };

                html! {
                    <button class={ class } onclick={ on_click } style={ key.css() }>{ label }</button>
                }
            }

        }).collect::<Html>();

        html! {
            <div class="row">{ row }</div>
        }
    }).collect()
}

struct Game {
    dictionary: Vec<GuessWord>,
    word: GuessWord,
    guesses: ArrayVec<[Guess; GUESSES]>,
    state: GameState,
    keyboard: Vec<Vec<Key>>,
    kbd_listener: Option<EventListener>,
}

impl Game {
    fn guess(&mut self) {
        log::debug!("{:?} is the target", self.word);
        let guess = self.current_guess().letters;
        let guess_str: AsciiString = guess.iter().map(|l| l.letter).collect();

        if self.state != GameState::Continue {
            log::debug!("State not continue");
            return;
        }

        if !self.dictionary.iter().any(|word| word.text == guess_str) {
            log::debug!("{} is not in dictionary", guess_str);
            return;
        }

        let mut result = [CharGuessResult::Incorrect; WORD_LENGTH];
        let mut remaining_letters: AsciiString = self.word.text.clone();

        log::debug!("First round of removals");

        let mut removal_cursor = 0;
        for idx in 0..WORD_LENGTH {
            if self.word.text[idx] == guess[idx].letter {
                result[idx] = CharGuessResult::Correct;
                remaining_letters.remove(removal_cursor);
            } else {
                removal_cursor += 1;
            }
        }

        log::debug!("{:?}", result);

        log::debug!("Second round of removals");

        for (idx, res) in result.iter_mut().enumerate().filter(|(_, res)| **res == CharGuessResult::Incorrect) {
            log::debug!("Index: {}. Result: {:?}. Guess: {}. Remaining: {}", idx, res, guess[idx].letter, remaining_letters);
            if let Some(idx) = remaining_letters.as_str().find(char::from(guess[idx].letter)) {
                log::debug!("  Found wrong place");
                *res = CharGuessResult::WrongPlace;
                remaining_letters.remove(idx);
            }
        }

        log::debug!("Assigning state");
        let guess = self.guesses.last_mut().unwrap();
        for (letter, state) in guess.letters.iter_mut().zip(result.iter()) {
            letter.state = Some(*state);

            for row in &mut self.keyboard {
                for key in row {
                    if key.key == letter.letter && key.state.map(|s| s < *state).unwrap_or(true) {
                        key.state = Some(*state);
                    }
                }
            }
        }

        if result.iter().all(|r| *r == CharGuessResult::Correct) {
            self.state = GameState::Won;
            return;
        }

        self.state = if self.guesses.len() == GUESSES {
            GameState::Lost
        } else {
            self.guesses.push(Guess::default());
            GameState::Continue
        };
    }

    fn current_guess(&mut self) -> &mut Guess {
        self.guesses.last_mut().unwrap()
    }
}

fn id<T>(v: T) -> T { v }

impl Component for Game {
    type Message = AsciiChar;
    type Properties = ();

    fn create(_ctx: &Context<Self>) -> Self {
        let list: Vec<_> = csv::Reader::from_reader(CSV)
            .deserialize::<WordRecord>()
            .flatten()
            .filter(|word| !word.is_plural)
            .collect();

        let process = |str: String| {
            iter::once(str)
                .filter(|s| s.len() == WORD_LENGTH)
                .map(|mut s| {
                    s.make_ascii_uppercase();
                    s
                })
                .filter_map(|s| s.into_ascii_string().ok())
                .next()
        };

        let dictionary: Vec<GuessWord> = list
            .into_iter()
            .map(|word| (word.word_id, word.xhosa))
            .filter_map(|(word_id, str)| process(str).map(|text| GuessWord { word_id, text }))
            .collect();

        let mut rng = rand::thread_rng();
        let word = dictionary.choose(&mut rng).unwrap().clone();

        let rows = ["QWERTYUIOP", "|ASDFGHJKL|", "\nZXCVBNM\x08"];
        let keys: Vec<Vec<Key>> = rows.into_iter().map(|row| {
            row.chars().map(|c| Key::from(AsciiChar::new(c))).collect()
        }).collect();

        let mut guesses = ArrayVec::default();

        guesses.push(Guess::default());

        Game {
            dictionary,
            word,
            guesses,
            state: GameState::Continue,
            keyboard: keys,
            kbd_listener: None
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: AsciiChar) -> bool {
        if self.state != GameState::Continue {
            return false;
        }

        log::debug!("{}", msg);

        match msg.into() {
            '\x08' => self.current_guess().letters.pop().is_some(),
            '\n' => {
                if self.current_guess().letters.len() == WORD_LENGTH {
                    self.guess();
                    true
                } else {
                    false
                }
            },
            _ => {
                let guess = &mut self.current_guess().letters;

                if guess.len() < WORD_LENGTH {
                    guess.push(msg.to_ascii_uppercase().into());
                    true
                } else {
                    false
                }
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let guesses_padded = self.guesses
            .iter()
            .copied()
            .chain(iter::repeat(Guess::default()))
            .take(GUESSES);

        let guesses = guesses_padded.map(|guess| {
            let letters_padded = guess
                .letters
                .iter()
                .copied()
                .chain(iter::repeat(GuessLetter::default()))
                .take(WORD_LENGTH);

            let guess = letters_padded.map(|letter| {
                html! { <div class="guess_letter" style={ letter.css() }>{ letter.letter }</div>}
            }).collect::<Html>();

            html! {
                <div class="guess">{ guess }</div>
            }
        }).collect::<Html>();

        html! {
            <>
                <h1>{ "Xhosa Wordle (beta)" }</h1>

                <div id="guesses">{ guesses }</div>

                <div id="keyboard">
                    <Keyboard keys={ self.keyboard.clone() } on_click={ ctx.link().callback(id) }/>
                </div>
            </>
        }
    }

    fn rendered(&mut self, ctx: &Context<Self>, first_render: bool) {
        if !first_render {
            return;
        }

        let document = gloo::utils::document();

        let callback = ctx.link().callback(|c: char| AsciiChar::new(c));
        let listener = EventListener::new(&document, "keydown", move |event| {
            let event = event.dyn_ref::<web_sys::KeyboardEvent>().unwrap();
            let first_char = event.key().chars().next().unwrap();

            let msg = match &event.key() as &str {
                "Backspace" => '\x08',
                "Enter" => '\n',
                _ if first_char.is_ascii_alphabetic() => first_char,
                _ => return,
            };

            callback.emit(msg);
        });

        self.kbd_listener.replace(listener);
    }
}

#[derive(Copy, Clone, PartialEq, Default)]
struct Guess {
    letters: ArrayVec<[GuessLetter; WORD_LENGTH]>,
}

#[derive(Copy, Clone, PartialEq)]
struct GuessLetter {
    state: Option<CharGuessResult>,
    letter: AsciiChar,
}

impl Default for GuessLetter {
    fn default() -> Self {
        GuessLetter {
            state: None,
            letter: AsciiChar::Space,
        }
    }
}

impl GuessLetter {
    fn css(&self) -> &'static str {
        self.state.map(|s| s.into_css()).unwrap_or_default()
    }
}

impl From<AsciiChar> for GuessLetter {
    fn from(letter: AsciiChar) -> Self {
        GuessLetter {
            state: None,
            letter,
        }
    }
}

fn main() {
    wasm_logger::init(wasm_logger::Config::default());
    log::info!("Starting in main_wrap");

    let wrap = gloo::utils::document().get_element_by_id("main_wrap");
    yew::start_app_in_element::<Game>(wrap.unwrap());
}

#[derive(Deserialize, Debug)]
struct WordRecord {
    word_id: u64,
    xhosa: String,
    is_plural: bool,
}

#[derive(Debug, Clone)]
struct GuessWord {
    word_id: u64,
    text: AsciiString,
}

#[derive(Eq, PartialEq, Debug, Copy, Clone, Ord, PartialOrd)]
enum CharGuessResult {
    Incorrect = 0,
    WrongPlace = 1,
    Correct = 2,
}

impl CharGuessResult {
    fn into_css(self) -> &'static str {
        use CharGuessResult::*;

        match self {
            Correct => "background-color: green",
            WrongPlace => "background-color: yellow",
            _ => "background-color: dimgray",
        }
    }
}

impl Display for CharGuessResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use CharGuessResult::*;

        let emoji = match self {
            Correct => '🟩',
            WrongPlace => '🟨',
            Incorrect => '⬛',
        };

        f.write_char(emoji)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum GameState {
    Won,
    Continue,
    Lost,
}

#[derive(Copy, Clone, Debug)]
struct GuessResult {
    typ: GameState,
    result: [CharGuessResult; WORD_LENGTH],
}

impl Display for GuessResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for c in self.result {
            write!(f, "{}", c)?;
        }

        match self.typ {
            GameState::Won => write!(f, "You have won the game!"),
            GameState::Lost => write!(f, "You have lost the game, better luck next time!"),
            GameState::Continue => Ok(()),
        }
    }
}
