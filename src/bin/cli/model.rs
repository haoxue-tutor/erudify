use chrono::{DateTime, Duration, Utc};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use serde_yaml;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use crate::convert::Exercise;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Proficiency {
    target_date: DateTime<Utc>,
    memory_strength: Duration,
}

impl Proficiency {
    pub fn fail(&mut self, at: DateTime<Utc>) {
        self.memory_strength = Duration::seconds(5);
        self.target_date = at + self.memory_strength;
    }

    pub fn success(&mut self, at: DateTime<Utc>) {
        if self.target_date > at {
            self.memory_strength += self.memory_strength / 50;
        } else {
            self.memory_strength += self.memory_strength * 4;
        }
        self.target_date = at + self.memory_strength;
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExerciseScore {
    // First priority: minimize words not in word_list
    pub words_not_in_list: usize,
    // Second priority: minimize words in word_list
    pub words_in_list: usize,
    // Third priority: minimize words not seen
    pub words_not_seen: usize,
    // Fourth priority: minimize last seen date (earlier is better)
    pub last_seen_date: Option<DateTime<Utc>>,
    // Fifth priority: minimize seen words with future target date
    pub future_words_count: usize,
}

pub struct WordListStatus {
    // Number of unique words in the word list
    pub total_words: usize,
    // Number of words with a repetition scheduled in the future
    pub known_words: usize,
    // Number of words with a repetition scheduled in the past
    pub words_to_review: usize,
    // Number of unique exercises that contain at least one word from the word list _and_ has been seen by the user.
    pub seen_sentences: usize,
    // Number of unique exercises that contain at least one word from the word list _and_ contains no unseen words.
    pub unlocked_sentences: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UserModel {
    seen_words: HashMap<String, Proficiency>,
    seen_exercises: HashMap<Exercise, DateTime<Utc>>,
}

impl UserModel {
    pub fn new() -> Self {
        Self {
            seen_words: HashMap::new(),
            seen_exercises: HashMap::new(),
        }
    }

    pub fn with_proficiency<'a>(
        &'a mut self,
        word: &str,
        now: DateTime<Utc>,
    ) -> &'a mut Proficiency {
        self.seen_words.entry(word.to_string()).or_insert({
            Proficiency {
                target_date: now,
                memory_strength: Duration::seconds(5),
            }
        })
    }

    pub fn seen(&self, word: &str) -> bool {
        self.seen_words.contains_key(word)
    }

    /// Load UserModel from a reader (generic over any Read type)
    pub fn load_from_reader<R: Read>(reader: R) -> Result<Self, Box<dyn std::error::Error>> {
        let model: UserModel = serde_yaml::from_reader(reader)?;
        Ok(model)
    }

    /// Save UserModel to a writer (generic over any Write type)
    pub fn save_to_writer<W: Write>(&self, writer: W) -> Result<(), Box<dyn std::error::Error>> {
        serde_yaml::to_writer(writer, self)?;
        Ok(())
    }

    /// Load UserModel from a YAML file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let file = fs::File::open(path)?;
        Self::load_from_reader(file)
    }

    /// Save UserModel to a YAML file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let file = fs::File::create(path)?;
        self.save_to_writer(file)
    }

    /// Load UserModel from the default application data directory
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let data_dir = Self::get_data_dir()?;
        let file_path = data_dir.join("user_model.yaml");
        Self::load_from_file(&file_path)
    }

    /// Save UserModel to the default application data directory
    pub fn store(&self) -> Result<(), Box<dyn std::error::Error>> {
        let data_dir = Self::get_data_dir()?;
        let file_path = data_dir.join("user_model.yaml");
        self.save_to_file(&file_path)
    }

    /// Get the application data directory, creating it if it doesn't exist
    fn get_data_dir() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
        let project_dirs = ProjectDirs::from("com", "erudify", "erudify")
            .ok_or("Failed to get project directories")?;

        let data_dir = project_dirs.data_dir();

        // Create the directory if it doesn't exist
        fs::create_dir_all(data_dir)?;

        Ok(data_dir.to_path_buf())
    }

    pub fn status(
        &self,
        exercises: &[Exercise],
        word_list: &[String],
        at: DateTime<Utc>,
    ) -> WordListStatus {
        let total_words = word_list.len();
        let known_words = self
            .seen_words
            .iter()
            .filter(|(word, prof)| word_list.contains(word) && prof.target_date > at)
            .count();
        let words_to_review = self
            .seen_words
            .iter()
            .filter(|(word, prof)| word_list.contains(word) && prof.target_date <= at)
            .count();

        let mut seen_sentences_set = HashSet::new();
        let mut unlocked_sentences_set = HashSet::new();

        for exercise in exercises {
            let exercise_words = exercise.words();
            if exercise_words.iter().any(|word| word_list.contains(word)) {
                if exercise_words
                    .iter()
                    .all(|word| self.seen_words.contains_key(word.as_str()))
                {
                    if self.seen_exercises.contains_key(exercise) {
                        seen_sentences_set.insert(exercise.clone());
                    }
                    unlocked_sentences_set.insert(exercise.clone());
                }
            }
        }

        WordListStatus {
            total_words,
            known_words,
            words_to_review,
            seen_sentences: seen_sentences_set.len(),
            unlocked_sentences: unlocked_sentences_set.len(),
        }
    }

    pub fn mark_seen(&mut self, exercise: &Exercise, at: DateTime<Utc>) {
        self.seen_exercises.insert(exercise.clone(), at);
    }

    /// Calculate the score for an exercise based on the prioritization criteria.
    /// Lower scores are better (we want to minimize each component in priority order).
    pub fn score_exercise(
        &self,
        now: DateTime<Utc>,
        exercise: &Exercise,
        word_list: &[String],
    ) -> ExerciseScore {
        let exercise_words = exercise.words();

        // Count future words (lowest priority)
        let future_words: HashSet<&&String> = exercise_words
            .iter()
            .filter(|word| {
                self.seen_words
                    .get(**word)
                    .map_or(false, |prof| prof.target_date > now)
            })
            .collect();

        // Count unseen words
        let words_not_seen = exercise_words
            .iter()
            .filter(|word| self.seen_words.get(**word).is_none())
            .count();

        // Among unseen words, count those in word_list vs not in word_list
        let words_in_list = exercise_words
            .iter()
            .filter(|word| !future_words.contains(word))
            .filter(|word| word_list.contains(**word))
            .count();

        let words_not_in_list = exercise_words
            .iter()
            .filter(|word| !future_words.contains(word))
            .filter(|word| !word_list.contains(**word))
            .count();

        // Get last seen date of the exercise
        let last_seen_date = self.seen_exercises.get(exercise).copied();

        ExerciseScore {
            words_not_in_list,
            words_in_list,
            words_not_seen,
            last_seen_date,
            future_words_count: future_words.len(),
        }
    }

    #[cfg(test)]
    /// Inserts a proficiency for a word such that its target date matches the given target date.
    /// This is useful for testing scenarios where you want to control exactly when a word is due.
    pub fn set_target_date(&mut self, word: &str, target_date: DateTime<Utc>) {
        self.seen_words.insert(
            word.to_string(),
            Proficiency {
                target_date,
                memory_strength: Duration::zero(),
            },
        );
    }

    // Must return a word that is in the word list.
    // Pick the seen word with the latest 'target_date' in the past.
    // If there's no such word, pick the next unseen word from the word_list.
    // If there are no unseen words, pick the seen word with the nearest 'target_date' in the future.
    pub fn next_word(&self, now: DateTime<Utc>, word_list: &[String]) -> String {
        #[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
        enum WordScore {
            // Seen, due for review (target_date <= now). Smaller diff is better (closer to now).
            SeenDue { diff: chrono::Duration },
            // Unseen word. Lower index is better.
            Unseen { idx: usize },
            // Seen, not yet due. Smaller diff is better (closer to now).
            SeenFuture { diff: chrono::Duration },
        }

        word_list
            .iter()
            .enumerate()
            .min_by_key(|(idx, word)| {
                if let Some(prof) = self.seen_words.get(*word) {
                    let target_date = prof.target_date;
                    let diff = target_date - now;
                    if diff <= chrono::Duration::zero() {
                        WordScore::SeenDue { diff: diff.abs() }
                    } else {
                        WordScore::SeenFuture { diff }
                    }
                } else {
                    WordScore::Unseen { idx: *idx }
                }
            })
            .expect("word_list must not be empty")
            .1
            .clone()
    }

    // Pick the exercise with the lowest cost.
    //
    // Exercises contain a set of words. We can split them into three categories:
    //  - Seen words with a target date in the future.
    //  - Words in the word_list.
    //  - Words not in the word_list.
    //
    // We want to pick an exercise such that:
    //  - It contains the target word.
    //  - We minimize the number of words not in the word_list (first priority).
    //  - We minimize the number of words in the word_list (second priority).
    //  - We minimize the last_seen_date of the exercise (third priority)
    //  - We minimize the number of seen words with a target date in the future (fourth priority).
    pub fn next_exercise(
        &self,
        now: DateTime<Utc>,
        exercises: &[Exercise],
        word_list: &[String],
        target_word: &str,
    ) -> Option<Exercise> {
        exercises
            .iter()
            .filter(|exercise| exercise.words().contains(&&target_word.to_string()))
            .min_by_key(|exercise| self.score_exercise(now, *exercise, word_list))
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper functions for creating specific UserModel instances

    /// Creates a simple word list for testing
    fn basic_word_list() -> Vec<String> {
        vec![
            "你好".to_string(),
            "谢谢".to_string(),
            "再见".to_string(),
            "学习".to_string(),
            "工作".to_string(),
        ]
    }

    /// Creates a fixed reference date for deterministic testing
    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2024-01-15T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn test_next_word_empty_model_returns_first_word() {
        assert_eq!(
            UserModel::new().next_word(now(), &basic_word_list()),
            basic_word_list()[0]
        );
    }

    #[test]
    fn test_next_word_seen_due_words_prioritized_by_closest_to_now() {
        let mut model = UserModel::new();
        model.set_target_date("你好", now() - Duration::hours(3));
        model.set_target_date("谢谢", now() - Duration::hours(2));
        model.set_target_date("再见", now() - Duration::hours(3));

        // "谢谢" is due closest to now (2 hours ago vs 3 hours ago)
        assert_eq!(model.next_word(now(), &basic_word_list()), "谢谢");
    }

    #[test]
    fn test_next_word_seen_future_words_prioritized_by_closest_to_now() {
        let mut model = UserModel::new();

        // Create words with different future due times (target_date > now)
        // Need to mark ALL words as seen to test the future word prioritization
        model.set_target_date("你好", now() + Duration::hours(5));
        model.set_target_date("谢谢", now() + Duration::hours(3));
        model.set_target_date("再见", now() + Duration::hours(7));
        model.set_target_date("学习", now() + Duration::hours(10));
        model.set_target_date("工作", now() + Duration::hours(15));

        // Since all words are seen and none are due, should pick the one due closest to now
        assert_eq!(model.next_word(now(), &basic_word_list()), "谢谢");
    }

    #[test]
    fn test_next_word_mixed_scenarios_prioritizes_due_over_unseen() {
        // Mix of due words, future words, and unseen words
        let mut model = UserModel::new();
        model.set_target_date("你好", now() - Duration::hours(2));
        model.set_target_date("谢谢", now() + Duration::hours(5));

        // "你好" should be prioritized because it's due for review
        let result = model.next_word(now(), &basic_word_list());
        assert_eq!(result, basic_word_list()[0]);
    }

    #[test]
    fn test_next_word_mixed_scenarios_prioritizes_unseen_over_future() {
        // Only future words (no due words)
        let mut model = UserModel::new();
        model.set_target_date("你好", now() + Duration::hours(5));
        model.set_target_date("谢谢", now() + Duration::hours(3));

        // Should return first unseen word since no words are due
        assert_eq!(
            model.next_word(now(), &basic_word_list()),
            basic_word_list()[2]
        );
    }

    #[test]
    fn test_next_word_all_words_seen_no_unseen() {
        let now = now();
        let word_list = vec!["你好".to_string(), "谢谢".to_string()];

        let mut model = UserModel::new();
        model.set_target_date("你好", now + Duration::hours(5));
        model.set_target_date("谢谢", now + Duration::hours(3));

        // All words are seen, so pick the one due closest to now
        let result = model.next_word(now, &word_list);
        assert_eq!(result, "谢谢");
    }

    #[test]
    fn test_next_word_exact_timing_edge_cases() {
        // Create words with very close timing
        let mut model = UserModel::new();
        model.set_target_date("你好", now() - Duration::seconds(1));
        model.set_target_date("谢谢", now() - Duration::seconds(2));

        // Both are due, but "你好" is due closer to now
        let result = model.next_word(now(), &basic_word_list());
        assert_eq!(result, "你好");
    }

    #[test]
    fn test_set_target_date() {
        let mut model = UserModel::new();

        // Insert a word that should be due 2 hours ago
        let target_date = now() - Duration::hours(2);
        model.set_target_date("你好", target_date);

        // Verify the proficiency was inserted correctly
        let proficiency = model.seen_words.get("你好").unwrap();
        assert_eq!(proficiency.target_date, target_date);

        // Insert another word that should be due in 3 hours
        let future_target_date = now() + Duration::hours(3);
        model.set_target_date("谢谢", future_target_date);

        // Verify the second proficiency was inserted correctly
        let proficiency2 = model.seen_words.get("谢谢").unwrap();
        assert_eq!(proficiency2.target_date, future_target_date);

        // Test that the word list prioritization works correctly
        let word_list = vec!["你好".to_string(), "谢谢".to_string()];
        let result = model.next_word(now(), &word_list);

        // "你好" should be prioritized because it's due (target_date <= now)
        assert_eq!(result, "你好");
    }

    #[test]
    fn test_next_word_with_single_word_list() {
        assert_eq!(
            UserModel::new().next_word(now(), &vec!["你好".to_string()]),
            "你好"
        );
    }

    #[test]
    #[should_panic(expected = "word_list must not be empty")]
    fn test_next_word_empty_word_list_panics() {
        UserModel::new().next_word(now(), &vec![]);
    }

    fn wo_shi_xuesheng_exercise() -> Exercise {
        let yaml = r#"
        segments:
          - chinese: 我
            pinyin: wǒ
          - chinese: 是
            pinyin: shì
          - chinese: 学生
            pinyin: xué sheng
          - chinese: 。
            pinyin: ''
        english: I am a student.
        "#;
        serde_yaml::from_str(yaml).expect("Failed to parse YAML into Exercise")
    }

    fn wo_xihuan_chi_jiaozi_exercise() -> Exercise {
        let yaml = r#"
        segments:
          - chinese: 我
            pinyin: wǒ
          - chinese: 喜欢
            pinyin: xǐ huan
          - chinese: 吃
            pinyin: chī
          - chinese: 饺子
            pinyin: jiǎo zi
          - chinese: 。
            pinyin: ''
        english: I like to eat dumplings.
        "#;
        serde_yaml::from_str(yaml).expect("Failed to parse YAML into Exercise")
    }

    #[test]
    fn test_next_exercise_empty_exercises_returns_none() {
        assert_eq!(
            UserModel::new().next_exercise(
                now(),
                &[],
                &["你好".to_string(), "谢谢".to_string()],
                "你好"
            ),
            None
        );
    }

    #[test]
    fn test_next_exercise_no_exercise_contains_target_word_returns_none() {
        let exercises = vec![wo_xihuan_chi_jiaozi_exercise(), wo_shi_xuesheng_exercise()];
        let word_list = vec!["你好".to_string(), "谢谢".to_string()];

        assert_eq!(
            UserModel::new().next_exercise(now(), &exercises, &word_list, "你好"),
            None
        );
    }

    #[test]
    fn test_next_exercise_prioritizes_least_words_not_in_list() {
        // Both exercises contain the target word "我".
        // The second exercise has fewer total words but more words in the word_list.
        // The first exercise has fewer words not in the word_list.
        let mut exercises = vec![wo_xihuan_chi_jiaozi_exercise(), wo_shi_xuesheng_exercise()];
        let word_list = vec!["我".to_string(), "喜欢".to_string(), "吃".to_string()];

        let result = UserModel::new()
            .next_exercise(now(), &exercises, &word_list, "我")
            .unwrap();
        assert_eq!(result, exercises[0]);

        // The order of the exercises should not matter.
        exercises.swap(0, 1);
        let result = UserModel::new()
            .next_exercise(now(), &exercises, &word_list, "我")
            .unwrap();
        assert_eq!(result, exercises[1]);
    }

    #[test]
    fn test_next_exercise_ignores_seen_future_words() {
        let exercises = vec![wo_xihuan_chi_jiaozi_exercise(), wo_shi_xuesheng_exercise()];
        let word_list = vec!["我".to_string(), "喜欢".to_string(), "吃".to_string()];

        let mut model = UserModel::new();
        model.set_target_date("是", now() + Duration::hours(2));
        model.set_target_date("学生", now() + Duration::hours(2));

        let result = model
            .next_exercise(now(), &exercises, &word_list, "我")
            .unwrap();
        assert_eq!(result, exercises[1]);
    }

    #[test]
    fn test_next_exercise_doesnt_ignore_seen_past_words() {
        let exercises = vec![wo_xihuan_chi_jiaozi_exercise(), wo_shi_xuesheng_exercise()];
        let word_list = vec!["我".to_string(), "喜欢".to_string(), "吃".to_string()];

        let mut model = UserModel::new();
        model.set_target_date("是", now() - Duration::hours(2));
        model.set_target_date("学生", now() - Duration::hours(2));

        let result = model
            .next_exercise(now(), &exercises, &word_list, "我")
            .unwrap();
        assert_eq!(result, exercises[0]);
    }

    #[test]
    fn test_score_exercise_1() {
        let word_list = vec!["我".to_string(), "喜欢".to_string(), "吃".to_string()];

        let mut model = UserModel::new();
        // Set "我" and "学生" as seen with past due dates
        model.set_target_date("我", now() - Duration::hours(2));
        model.set_target_date("喜欢", now() - Duration::hours(2));

        // Test the scoring method directly
        let score = model.score_exercise(now(), &wo_xihuan_chi_jiaozi_exercise(), &word_list);

        assert_eq!(score.future_words_count, 0);
        assert_eq!(score.words_in_list, 3); // "我", "喜欢", "吃"
        assert_eq!(score.words_not_in_list, 1); // "饺子"

        // Set "我" and "喜欢" as known (seen + future due date)
        model.set_target_date("我", now() + Duration::hours(2));
        model.set_target_date("喜欢", now() + Duration::hours(2));

        // Test the scoring method directly
        let score = model.score_exercise(now(), &wo_xihuan_chi_jiaozi_exercise(), &word_list);

        assert_eq!(score.future_words_count, 2);
        assert_eq!(score.words_in_list, 1);
        assert_eq!(score.words_not_in_list, 1);
    }

    #[test]
    fn test_serialization_roundtrip() {
        // Create a UserModel with some data
        let mut model = UserModel::new();
        model.set_target_date("你好", now() + Duration::hours(1));
        model.set_target_date("谢谢", now() + Duration::hours(2));

        // Serialize to YAML
        let yaml = serde_yaml::to_string(&model).expect("Failed to serialize");

        // Deserialize back
        let deserialized: UserModel = serde_yaml::from_str(&yaml).expect("Failed to deserialize");

        // Verify they're equal
        assert_eq!(model, deserialized);
    }

    #[test]
    fn test_load_save_file() {
        // Create a temporary file that will be automatically cleaned up
        let temp_file = tempfile::NamedTempFile::new().expect("Failed to create temp file");

        // Create a UserModel with some data
        let mut model = UserModel::new();
        model.set_target_date("测试", now() + Duration::hours(3));

        // Save to file
        model
            .save_to_file(temp_file.path())
            .expect("Failed to save");

        // Load from file
        let loaded = UserModel::load_from_file(temp_file.path()).expect("Failed to load");

        // Verify they're equal
        assert_eq!(model, loaded);

        // Temp file is automatically cleaned up when it goes out of scope
    }

    #[test]
    fn test_load_from_reader_with_string() {
        // Create a UserModel with some data
        let mut model = UserModel::new();
        model.set_target_date("你好", now() + Duration::hours(1));
        model.set_target_date("谢谢", now() + Duration::hours(2));

        // Serialize to YAML string
        let yaml = serde_yaml::to_string(&model).expect("Failed to serialize");

        // Load from string reader
        let loaded =
            UserModel::load_from_reader(yaml.as_bytes()).expect("Failed to load from reader");

        // Verify they're equal
        assert_eq!(model, loaded);
    }

    #[test]
    fn test_save_to_writer_with_buffer() {
        use std::io::Cursor;

        // Create a UserModel with some data
        let mut model = UserModel::new();
        model.set_target_date("测试", now() + Duration::hours(3));

        // Save to buffer
        let mut buffer = Vec::new();
        model
            .save_to_writer(&mut buffer)
            .expect("Failed to save to writer");

        // Load from buffer
        let loaded =
            UserModel::load_from_reader(Cursor::new(buffer)).expect("Failed to load from buffer");

        // Verify they're equal
        assert_eq!(model, loaded);
    }

    #[test]
    fn test_reader_writer_demo() {
        use std::io::Cursor;

        // Create a UserModel with some data
        let mut model = UserModel::new();
        model.set_target_date("你好", now() + Duration::hours(1));
        model.set_target_date("谢谢", now() + Duration::hours(2));

        // Demonstrate different reader/writer patterns

        // 1. String to String (full roundtrip)
        let yaml_string = serde_yaml::to_string(&model).unwrap();
        let loaded_from_string = UserModel::load_from_reader(yaml_string.as_bytes()).unwrap();
        assert_eq!(model, loaded_from_string);

        // 2. Buffer to Buffer (streaming)
        let mut buffer = Vec::new();
        model.save_to_writer(&mut buffer).unwrap();
        let loaded_from_buffer = UserModel::load_from_reader(Cursor::new(buffer)).unwrap();
        assert_eq!(model, loaded_from_buffer);

        // 3. File to File (traditional)
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        model.save_to_file(temp_file.path()).unwrap();
        let loaded_from_file = UserModel::load_from_file(temp_file.path()).unwrap();
        assert_eq!(model, loaded_from_file);
    }
}
