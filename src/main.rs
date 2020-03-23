use actix_web::{web, App, HttpServer, Responder};

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::sync::Mutex;

const WORD_LEN: usize = 5;
type CharFrequency = HashMap<char, Vec<u32>>;
type AppState = Mutex<State>;

#[derive(Clone, Debug)]
struct Word {
    word: String,
}

impl Word {
    fn has(&self, clue: &Clue) -> bool {
        let mut occur = 0;
        for (idx, c) in self.word.chars().enumerate() {
            if c == clue.c {
                occur += 1;
            }
            match clue.hints[idx] {
                Hint::Yes => {
                    if c != clue.c  {
                        return false;
                    }
                },
                Hint::No => {
                    if c == clue.c {
                        return false;
                    }
                },
                _ => {},
            }
        }
        if occur < clue.occur {
            return false;
        }
        true
    }

    fn score(&self, freq: &CharFrequency) -> u32 {
        let mut chars: Vec<char> = self.word.chars().collect();
        chars.sort();
        chars.dedup();

        let mut score = 0;

        for c in chars {
            if let Some(f) = freq.get(&c) {
                for (idx, count) in f.iter().enumerate() {
                    if self.word.chars().nth(idx).unwrap() == c {
                        score += count * 4;
                    }
                    else {
                        score += count;
                    }
                }
            }
        }

        score
    }
}

#[derive(Clone, Debug)]
struct Dictionary {
    words: Vec<Word>,
    ignore_letters: Vec<char>,
}

impl Dictionary {
    fn filter(&mut self, clue: &Clue) {
        let mut certain = true;
        for hint in &clue.hints {
            if let Hint::Maybe = hint {
                certain = false;
                break;
            }
        }

        if certain {
            self.ignore_letters.push(clue.c);
        }

        self.words = self.words.clone().into_iter()
            .filter(|w| w.has(clue))
            .collect();
    }

    fn sort(&mut self, freq: &CharFrequency) -> Word {
        self.words.sort_by_cached_key(|w| -(w.score(freq) as i64));
        self.words[0].clone()
    }

    fn from_file(mut file: File) -> Dictionary {
        let mut data = String::new();
        file.read_to_string(&mut data)
            .expect("Error reading dictionary file.");

        let mut words = Vec::new();

        for line in data.lines() {
            let word = line.trim().to_string();

            if word.len() == WORD_LEN {
                words.push(Word{word});
            }
        }

        Dictionary {
            words,
            ignore_letters: Vec::new(),
        }
    }

    fn empty() -> Dictionary {
        Dictionary {
            words: Vec::new(),
            ignore_letters: Vec::new(),
        }
    }

    fn char_frequency(&self) -> CharFrequency {
        let mut freq: CharFrequency = HashMap::new();

        for word in &self.words {
            for (idx, c) in word.word.char_indices() {
                match freq.get_mut(&c)  {
                    Some(f) => f[idx] += 1,
                    None => {
                        let mut pos = vec![0; WORD_LEN];
                        pos[idx] = 1;
                        freq.insert(c, pos);
                    },
                }
            }
        }

        for c in &self.ignore_letters {
            if let Some(v) = freq.get_mut(c) {
                *v = vec![0; WORD_LEN];
            }
        }

        freq
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Hint {
    Yes,
    No,
    Maybe,
    Unset,
}

#[derive(Clone, Debug)]
struct Clue {
    c: char,
    occur: u32,
    hints: Vec<Hint>
}

impl Clue {
    fn from_input(guess: &str, inp: &str) -> Vec<Clue> {
        let mut chars: Vec<char> = guess.chars().collect();
        chars.sort();
        chars.dedup();

        let input_chars: Vec<char> = inp.chars().collect();

        let mut clues = Vec::new();
        
        for c in chars {
            let matches = guess.match_indices(c);
            let mut hints = vec![Hint::Unset; WORD_LEN];

            let mut correct = 0;
            let mut wrong_place = 0;
            let mut wrong = 0;

            for (idx, _) in matches {
                if input_chars[idx] == 'c' {
                    correct += 1;
                    hints[idx] = Hint::Yes;
                }
                else if input_chars[idx] == 'w' {
                    wrong_place += 1;
                    hints[idx] = Hint::No;
                }
                else {
                    wrong += 1;
                    hints[idx] = Hint::No;
                }
            }

            let mut replace = Hint::Maybe;
            if wrong > 0 {
                replace = Hint::No;
            }

            for hint in &mut hints {
                if *hint == Hint::Unset {
                    *hint = replace.clone();
                }
            }

            let mut occur = 0;
            occur += correct;
            if wrong_place > 0 {
                occur += 1;
            }

            clues.push(Clue {
                c,
                occur,
                hints,
            });
        }

        clues
    }
}

#[derive(Clone)]
struct State {
    all_words: Dictionary,
    valid_words: Dictionary,
    valid_guesses: Dictionary,
}

fn get_guess(state: &mut State) -> String {
    let len = state.valid_words.words.len();
    if len == 0 {
        return String::from("No possible words!");
    }
    else if len == 1{
        return state.valid_words.words[0].word.clone();
    }

    let freq = state.valid_words.char_frequency();
    state.valid_guesses.sort(&freq).word
}

async fn reset(path: web::Path<(char,)>, state: web::Data<AppState>) -> impl Responder {
    let mut state = state.lock().expect("Error locking mutex");

    let mut words = state.all_words.clone();
    words.filter(&Clue {
        c: path.0,
        occur: 1,
        hints: vec![Hint::Yes, Hint::Maybe, Hint::Maybe, Hint::Maybe, Hint::Maybe]
    });

    state.valid_words = words.clone();
    state.valid_guesses = words;

    get_guess(&mut state)
}

async fn hint(path: web::Path<(String, String)>, state: web::Data<AppState>) -> impl Responder {
    let mut state = state.lock().expect("Error locking mutex");

    let clues = Clue::from_input(&path.0, &path.1);
    for clue in clues {
        state.valid_words.filter(&clue);
    }

    get_guess(&mut state)
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    let state = web::Data::new(Mutex::new(State {
        all_words: Dictionary::from_file(File::open("words_alpha.txt").expect("Error opening dict file")),
        valid_words: Dictionary::empty(),
        valid_guesses: Dictionary::empty(),
    }));
    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .route("/reset/{letter}", web::get().to(reset))
            .route("/hint/{word}/{hint}", web::get().to(hint))
    })
    .bind("0.0.0.0:8088")?
    .run()
    .await
}

/*
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut valid_words = Dictionary::from_file(File::open("words_alpha.txt")?);

    let c = input::<char>().msg("What is the first character? ").get()
        .to_lowercase()
        .next()
        .expect("Error converting to lowercase");

    valid_words.filter(&Clue {
        c,
        occur: 1,
        hints: vec![Hint::Yes, Hint::Maybe, Hint::Maybe, Hint::Maybe, Hint::Maybe]
    });

    let mut possible_guesses = valid_words.clone();

    loop {
        let len = valid_words.words.len();
        if len == 0 {
            println!("No more possible words! Did you make a mistake?");
            break;
        }
        else if len == 1{
            println!("I got it! Your word is: {}", valid_words.words[0].word);
            break;
        }

        let freq = valid_words.char_frequency();
        let guess = possible_guesses.sort(&freq);
        println!("I guess {}", guess.word);

        let inp = input::<String>().msg("What is your hint? ").get();

        let clues = Clue::from_input(&guess.word, &inp);
        for clue in clues {
            valid_words.filter(&clue);
        }
    }

    Ok(())
}*/
