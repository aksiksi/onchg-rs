use std::path::PathBuf;
use std::{io::Write, path::Path};

use base64::Engine;
use rand::{RngCore, SeedableRng};

use crate::{OnChangeBlock, ThenChange, ThenChangeTarget};

pub struct RandomOnChangeTree {
    root: PathBuf,
    rng: rand::rngs::StdRng,
    b64: base64::engine::GeneralPurpose,
    directories: Vec<PathBuf>,
    blocks: Vec<(PathBuf, OnChangeBlock)>,
    max_directory_depth: usize,
    min_blocks_per_file: usize,
    max_blocks_per_file: usize,
    max_lines_per_block: usize,
    max_file_line_length: usize,
}

impl RandomOnChangeTree {
    pub fn new(
        root: PathBuf,
        seed: u64,
        max_directory_depth: usize,
        min_blocks_per_file: usize,
        max_blocks_per_file: usize,
        max_lines_per_block: usize,
        max_file_line_length: usize,
    ) -> Self {
        let mut raw_seed = [0u8; 32];
        raw_seed[0..8].copy_from_slice(&seed.to_le_bytes());
        let rng = rand::rngs::StdRng::from_seed(raw_seed);
        let b64 = base64::engine::GeneralPurpose::new(
            &base64::alphabet::URL_SAFE,
            base64::engine::GeneralPurposeConfig::new(),
        );
        Self {
            root,
            rng,
            b64,
            directories: Vec::new(),
            blocks: Vec::new(),
            max_directory_depth,
            min_blocks_per_file,
            max_blocks_per_file,
            max_lines_per_block,
            max_file_line_length,
        }
    }

    pub fn init(&mut self, num_directories: usize, num_files: usize) {
        for _ in 0..num_directories {
            self.create_directory();
        }
        for _ in 0..num_files {
            self.create_file();
        }
    }

    fn next_string(&mut self) -> String {
        let mut s = self.b64.encode(self.rng.next_u64().to_le_bytes());
        s.truncate(s.len() - 1);
        s
    }

    fn rand_string(&mut self, len: Option<usize>) -> String {
        let mut s = self.next_string();
        if let Some(len) = len {
            while s.len() < len {
                s.push_str(&self.next_string());
            }
            s.truncate(len);
        }
        s
    }

    fn rand_le(&mut self, max: usize) -> usize {
        self.rng.next_u32() as usize % max
    }

    fn rand_in_range(&mut self, min: usize, max: usize) -> usize {
        self.rng.next_u32() as usize % (max - min) + min
    }

    fn rand_bool(&mut self) -> bool {
        self.rand_le(2) == 0
    }

    // Lifetimes are tricky with this one...
    #[allow(unused)]
    fn rand_elem<'a, T>(&mut self, elems: &'a [T]) -> &'a T {
        &elems[self.rand_le(elems.len())]
    }

    fn create_directory(&mut self) {
        let mut depth = self.rand_le(self.max_directory_depth + 1);

        // If we have existing directories, we should randomly try to choose one as a parent.
        let mut parent: Option<PathBuf> = None;
        if self.directories.len() > 0 && self.rand_bool() {
            // This attempt will fail if the parent's depth is equal to the max depth.
            // In this case, we simply fallback to the normal flow.
            let n = self.rand_le(self.directories.len());
            let p = &self.directories[n];
            let parent_depth = p.components().collect::<Vec<_>>().len();
            if parent_depth < self.max_directory_depth {
                depth = self.max_directory_depth - parent_depth;
                parent = Some(p.to_owned());
            }
        }

        let parts = (0..depth)
            .into_iter()
            .map(|_| self.rand_string(None))
            .collect::<Vec<String>>();
        let p = PathBuf::from_iter(parts.into_iter());
        let p = if let Some(parent) = parent {
            parent.join(p)
        } else {
            p
        };

        std::fs::create_dir_all(self.root.join(&p)).unwrap();

        self.directories.push(p);
    }

    fn create_file(&mut self) {
        let n = self.rand_le(self.directories.len());
        let file_name = format!("{}.file", self.rand_string(None));
        let d = &self.directories[n];
        let path = d.join(file_name);
        let mut f = std::fs::File::create(self.root.join(&path)).unwrap();
        let blocks = self.create_blocks(path.clone(), &mut f);
        for block in blocks {
            self.blocks.push((path.clone(), block));
        }
    }

    fn targetable_blocks(&self) -> Vec<(&Path, &OnChangeBlock)> {
        self.blocks
            .iter()
            .filter_map(|(p, b)| {
                if b.is_targetable() {
                    Some((p.as_path(), b))
                } else {
                    None
                }
            })
            .collect()
    }

    fn block_to_strings(block: &OnChangeBlock) -> (String, String) {
        let on_change_string = format!("LINT.OnChange({})\n", block.name_raw().unwrap_or(""));

        let then_change_target = match block.then_change() {
            ThenChange::Targets(targets) => targets
                .into_iter()
                .map(|t| {
                    let target_file = t
                        .file()
                        .as_ref()
                        .map(|p| format!("//{}", p.to_str().unwrap()))
                        .unwrap_or("".to_string());
                    let target_block = t.block();
                    if let Some(target_block) = target_block {
                        format!("{}:{}", target_file, target_block)
                    } else {
                        format!("{}", target_file)
                    }
                })
                .collect::<Vec<String>>()
                .join(","),
            ThenChange::NoTarget => "".to_string(),
            ThenChange::Unset => unreachable!(),
        };
        let then_change_string = format!("LINT.ThenChange({})\n", then_change_target);

        (on_change_string, then_change_string)
    }

    fn create_blocks(&mut self, path: PathBuf, f: &mut std::fs::File) -> Vec<OnChangeBlock> {
        let mut blocks: Vec<OnChangeBlock> = Vec::new();

        let mut content = String::new();

        let num_blocks = self.rand_in_range(self.min_blocks_per_file, self.max_blocks_per_file + 1);
        let mut line_num = 0;
        for _ in 0..num_blocks {
            let num_lines = self.rand_le(self.max_lines_per_block);
            let block_name = if self.rand_bool() {
                Some(self.rand_string(None))
            } else {
                None
            };

            let mut then_change_file: Option<PathBuf> = None;
            let mut then_change_block: Option<String> = None;

            let chosen = self.rand_bool();
            if chosen && self.blocks.len() > 0 {
                // Target an existing file + block.
                let (p, b) = {
                    let target_blocks = self.targetable_blocks();
                    let r = self.rand_le(target_blocks.len());
                    let b = self.targetable_blocks()[r].clone();
                    (b.0.to_owned(), b.1.to_owned())
                };
                then_change_file = Some(p);
                then_change_block = if self.rand_le(100) < 25 {
                    // 25% chance to only use a file target.
                    None
                } else {
                    Some(b.name().to_string())
                };
            } else if !chosen {
                // 50% to target existing in-file block (assuming we have one that is targetable!).
                for b in &blocks {
                    if b.is_targetable() {
                        then_change_block = Some(b.name().to_string())
                    }
                }
            }

            let start_line = line_num as u32;
            let end_line = (line_num + num_lines) as u32;
            let block_target: ThenChange = match (then_change_file, then_change_block) {
                (then_change_file, Some(then_change_block)) => ThenChangeTarget::Block {
                    block: then_change_block,
                    file: then_change_file,
                }
                .into(),
                (Some(then_change_file), None) => ThenChangeTarget::File(then_change_file).into(),
                (None, None) => ThenChange::NoTarget,
            };
            let block =
                OnChangeBlock::new(path.clone(), block_name, start_line, end_line, block_target);

            let (on_change_string, then_change_string) = Self::block_to_strings(&block);

            content.push_str(&on_change_string);
            for _ in 0..num_lines {
                let n = self.rand_le(self.max_file_line_length);
                let line_content = self.rand_string(Some(n));
                content.push_str(&line_content);
                content.push('\n');
            }
            content.push_str(&then_change_string);

            blocks.push(block);

            line_num += num_lines + 1;
        }

        f.write(content.as_bytes()).unwrap();

        blocks
    }

    pub fn touch_random_block(&mut self) {
        let n = self.rand_le(self.targetable_blocks().len());
        let (p, b) = self.targetable_blocks()[n];
        let start_line = b.start_line() as usize;
        let p = self.root.join(p);

        let mut f = std::fs::File::options().write(true).open(&p).unwrap();
        let s = std::fs::read_to_string(&p).unwrap();
        let mut lines: Vec<&str> = s.lines().collect();

        let mut insert_after = None;
        for (n, _) in lines.iter().enumerate() {
            if n + 1 == start_line {
                insert_after = Some(n);
            }
        }
        if let Some(insert_after) = insert_after {
            lines.insert(insert_after, "some change!");
        }

        f.write_all(lines.join("\n").as_bytes()).unwrap();
    }
}
