use std::fmt;

const HIGH: [char; 128] = [
    '\u{410}', '\u{411}', '\u{412}', '\u{413}', '\u{414}', '\u{415}', '\u{416}', '\u{417}',
    '\u{418}', '\u{419}', '\u{41a}', '\u{41b}', '\u{41c}', '\u{41d}', '\u{41e}', '\u{41f}',
    '\u{420}', '\u{421}', '\u{422}', '\u{423}', '\u{424}', '\u{425}', '\u{426}', '\u{427}',
    '\u{428}', '\u{429}', '\u{42a}', '\u{42b}', '\u{42c}', '\u{42d}', '\u{42e}', '\u{42f}',
    '\u{430}', '\u{431}', '\u{432}', '\u{433}', '\u{434}', '\u{435}', '\u{436}', '\u{437}',
    '\u{438}', '\u{439}', '\u{43a}', '\u{43b}', '\u{43c}', '\u{43d}', '\u{43e}', '\u{43f}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}', '\u{2556}',
    '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}', '\u{255d}', '\u{255c}', '\u{255b}', '\u{2510}',
    '\u{2514}', '\u{2534}', '\u{252c}', '\u{251c}', '\u{2500}', '\u{253c}', '\u{255e}', '\u{255f}',
    '\u{255a}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256c}', '\u{2567}',
    '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}', '\u{2558}', '\u{2552}', '\u{2553}', '\u{256b}',
    '\u{256a}', '\u{2518}', '\u{250c}', '\u{2588}', '\u{2584}', '\u{258c}', '\u{2590}', '\u{2580}',
    '\u{440}', '\u{441}', '\u{442}', '\u{443}', '\u{444}', '\u{445}', '\u{446}', '\u{447}',
    '\u{448}', '\u{449}', '\u{44a}', '\u{44b}', '\u{44c}', '\u{44d}', '\u{44e}', '\u{44f}',
    '\u{401}', '\u{451}', '\u{404}', '\u{454}', '\u{407}', '\u{457}', '\u{40e}', '\u{45e}',
    '\u{b0}', '\u{2219}', '\u{b7}', '\u{221a}', '\u{2116}', '\u{a4}', '\u{25a0}', '\u{a0}',
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodingError {
    pub character: char,
    pub character_index: usize,
}

impl fmt::Display for EncodingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "character {:?} at position {} is not representable in CP866",
            self.character, self.character_index
        )
    }
}
impl std::error::Error for EncodingError {}

pub fn decode_cp866(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&byte| {
            if byte < 128 {
                char::from(byte)
            } else {
                HIGH[usize::from(byte - 128)]
            }
        })
        .collect()
}

pub fn encode_cp866(text: &str) -> Result<Vec<u8>, EncodingError> {
    text.chars()
        .enumerate()
        .map(|(index, character)| {
            if character.is_ascii() {
                Ok(character as u8)
            } else {
                HIGH.iter()
                    .position(|&mapped| mapped == character)
                    .map(|position| position as u8 + 128)
                    .ok_or(EncodingError {
                        character,
                        character_index: index + 1,
                    })
            }
        })
        .collect()
}
