use array_init::array_init;
use rayon::prelude::*;
use std::fs;

/// A clue block
#[derive(PartialEq, Eq)]
enum Info {
    Black,
    Yellow,
    Green,
}

impl Info {
    /// Decode a clue block from a character from stdin.
    fn from_u8(c: u8) -> Info {
        match c as char {
            'x' | 'b' => Info::Black,
            'g' => Info::Green,
            'y' => Info::Yellow,
            c => panic!("Info::from_u8: {}", c),
        }
    }
}

/// A clue consists of 5 pieces of info.
type Clue = [Info; 5];

const A: u8 = 'a' as u8;

#[derive(PartialEq, Eq, Clone, Copy)]
enum LetterStatus {
    Yes,
    No,
    Unknown,
}

/// The state of our knowledge about each letter.
/// Either it's definitely in the word, definitely not in the word,
/// or it's not yet known which.
#[derive(PartialEq, Eq, Clone, Copy)]
struct LettersContained([LetterStatus; 26]);

impl LettersContained {
    /// Check whether a letter is potentially in the word.
    #[inline(always)]
    fn possibly_contains(&self, x: u8) -> bool {
        match self.0[u8_to_letter_index(x)] {
            LetterStatus::No => false,
            LetterStatus::Unknown | LetterStatus::Yes => true,
        }
    }

    #[inline(always)]
    fn mark_contains(&mut self, x: u8) {
        self.0[u8_to_letter_index(x)] = LetterStatus::Yes
    }

    #[inline(always)]
    fn mark_not_contains(&mut self, x: u8) {
        self.0[u8_to_letter_index(x)] = LetterStatus::No
    }
}

/// The state of our knowledge about the answer.
#[derive(PartialEq, Eq)]
struct InfoState {
    /// The letters that we know for sure.
    /// The non-alphabetic code point 0 is used for unknown spots.
    mask: [u8; 5],
    /// The set of letters that we know appear in the answer.
    letters: LettersContained,
}

#[inline(always)]
fn u8_to_letter_index(x: u8) -> usize {
    (x - A) as usize
}

impl InfoState {
    /// Check whether a word is a possible answer given the current
    /// state of our information.
    fn consistent(&self, w: &Word) -> bool {
        let mask = &self.mask;
        for i in 0..5 {
            if mask[i] == 0 {
                if !self.letters.possibly_contains(w[i]) {
                    return false;
                }
            } else {
                if w[i] != mask[i] {
                    return false;
                }
            }
        }
        return true;
    }

    /// Update the state of our knowledge given the guess we made
    /// and the clue that we were given.
    fn update(&self, guess: Word, clue: Clue) -> Self {
        let mask = array_init(|i| {
            if self.mask[i] != 0 {
                self.mask[i]
            } else {
                if clue[i] == Info::Green {
                    guess[i]
                } else {
                    0
                }
            }
        });

        let mut letters = self.letters;
        for i in 0..5 {
            match clue[i] {
                Info::Green | Info::Yellow => letters.mark_contains(guess[i]),
                Info::Black => letters.mark_not_contains(guess[i]),
            }
        }

        InfoState { letters, mask }
    }

    /// Update the state of our knowledge given the guess we made and the
    /// actual answer.
    fn update_from_answer(&self, guess: &Word, answer: &Word) -> Self {
        let mut letters = self.letters;
        // Set to true all chars in the guess that are also in the answer
        for i in 0..5 {
            let c = guess[i];
            let answer_has_c = c == answer[0]
                || c == answer[1]
                || c == answer[2]
                || c == answer[3]
                || c == answer[4];
            if answer_has_c {
                letters.mark_contains(c)
            } else {
                letters.mark_not_contains(c);
            }
        }

        InfoState {
            letters,
            mask: array_init(|i| {
                if self.mask[i] != 0 {
                    self.mask[i]
                } else {
                    if guess[i] == answer[i] {
                        guess[i]
                    } else {
                        0
                    }
                }
            }),
        }
    }

    fn new() -> Self {
        InfoState {
            letters: LettersContained(array_init(|_| LetterStatus::Unknown)),
            mask: array_init(|_| 0),
        }
    }
}

/// A word is 5 bytes
type Word = [u8; 5];

/// Given a guess and the actual hidden answer, how many words
/// would remain after the corresponding clue is given
fn remaining_possibilities(
    info: &InfoState,
    guess: &Word,
    actual_answer: &Word,
    possible_answers: &Vec<Word>,
) -> usize {
    let info = info.update_from_answer(guess, actual_answer);
    possible_answers
        .par_iter()
        .filter(|a| info.consistent(a))
        .count()
}

/// Assuming a uniform distribution over the set of possible_answers,
/// compute the expected number of remaining possibilities for a given
/// guess, times the total number of possible answers.
///
/// It's easier to compute without dividing by the number of possible
/// answers. Doing so would yield the actual expected number, but it's
/// not necssary since we use this as a "score" of how bad a guess is.
fn unnormalized_expected_remaining_possibilities(
    info: &InfoState,
    guess: &Word,
    possible_answers: &Vec<Word>,
) -> usize {
    possible_answers
        .par_iter()
        .map(|a| remaining_possibilities(info, guess, a, possible_answers))
        .sum()
}

fn main() {
    let words: Vec<Word> = {
        let s = fs::read("words").unwrap();
        let mut ws = vec![];

        let mut i = 0;
        loop {
            ws.push(array_init(|j| s[i + j]));
            i += 6;
            if i >= s.len() {
                break;
            }
        }
        ws
    };

    let mut info = InfoState::new();
    let mut possible_answers = words.clone();

    let best_initial_guess = "lares";

    fn read_clue() -> Clue {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap(); // including '\n'
        let line = line.as_bytes();
        array_init(|i| Info::from_u8(line[i]))
    }

    let mut last_guess: Word = array_init(|i| best_initial_guess.as_bytes()[i]);
    loop {
        let clue = read_clue();
        info = info.update(last_guess, clue);
        possible_answers = possible_answers
            .par_iter()
            .filter(|a| info.consistent(a))
            .map(|a| *a)
            .collect();

        // find the word which yields the largest reduction in the number of consistent words

        let scored_guesses: Vec<(Word, usize)> = possible_answers
            .iter()
            .map(|guess| {
                let score =
                    unnormalized_expected_remaining_possibilities(&info, guess, &possible_answers);
                println!("score {} = {}", std::str::from_utf8(guess).unwrap(), score);
                (*guess, score)
            })
            .collect();

        let next_guess = scored_guesses.par_iter().min_by_key(|(_, s)| *s).unwrap().0;

        println!("guess: {}", std::str::from_utf8(&next_guess).unwrap());
        last_guess = next_guess;
    }
}

fn clue(guess: Word, answer: Word) -> Clue {
    array_init(|i| {
        if guess[i] == answer[i] {
            Info::Green
        } else if answer.contains(&guess[i]) {
            Info::Yellow
        } else {
            Info::Black
        }
    })
}
