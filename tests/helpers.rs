use std::{io::Write, path::PathBuf};

use base64::Engine;
use rand::{RngCore, SeedableRng};

pub struct RandomOnChangeTree {
    root: PathBuf,
    rng: rand::rngs::StdRng,
    b64: base64::engine::GeneralPurpose,
    directories: Vec<PathBuf>,
    // TODO(aksiksi): Keep track of block locations and targets to allow tests
    // to modify specific blocks for Git-based tests and benches.
    //
    // In fact, we could probably just build OnChangeBlocks and serialize them to
    // strings when building blocks in a file.
    blocks: Vec<(PathBuf, String)>,
    max_directory_depth: usize,
    max_blocks_per_file: usize,
    max_lines_per_block: usize,
}

impl RandomOnChangeTree {
    pub fn new(
        root: PathBuf,
        seed: u64,
        max_directory_depth: usize,
        max_blocks_per_file: usize,
        max_lines_per_block: usize,
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
            max_blocks_per_file,
            max_lines_per_block,
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

    fn rand_string(&mut self) -> String {
        let s = self.b64.encode(self.rng.next_u64().to_le_bytes());
        String::from(&s[..s.len() - 1])
    }

    fn rand_in_range(&mut self, max: usize) -> usize {
        self.rng.next_u32() as usize % max
    }

    fn rand_bool(&mut self) -> bool {
        self.rand_in_range(2) == 0
    }

    fn create_directory(&mut self) {
        let mut depth = self.rand_in_range(self.max_directory_depth + 1);

        // If we have existing directories, we should randomly try to choose one as a parent.
        let mut parent: Option<PathBuf> = None;
        if self.directories.len() > 0 && self.rand_bool() {
            // This attempt will fail if the parent's depth is equal to the max depth.
            // In this case, we simply fallback to the normal flow.
            let n = self.rand_in_range(self.directories.len());
            let p = &self.directories[n];
            let parent_depth = p.components().collect::<Vec<_>>().len();
            if parent_depth < self.max_directory_depth {
                depth = self.max_directory_depth - parent_depth;
                parent = Some(p.to_owned());
            }
        }

        let parts = (0..depth)
            .into_iter()
            .map(|_| self.rand_string())
            .collect::<Vec<String>>();
        let mut p = PathBuf::from_iter(parts.into_iter());

        if let Some(parent) = parent {
            p = parent.join(p);
        } else {
            p = self.root.join(p);
        }

        std::fs::create_dir_all(&p).unwrap();

        self.directories.push(p);
    }

    fn create_file(&mut self) {
        let n = self.rand_in_range(self.directories.len());
        let file_name = format!("{}.file", self.rand_string());
        let d = &self.directories[n];
        let path = d.join(file_name);
        let mut f = std::fs::File::create(&path).unwrap();
        let blocks = self.create_blocks(&mut f);
        for block in blocks {
            self.blocks.push((path.clone(), block));
        }
    }

    fn create_blocks(&mut self, f: &mut std::fs::File) -> Vec<String> {
        let mut blocks: Vec<String> = Vec::new();
        let blocks_len = self.blocks.len();

        let num_blocks = self.rand_in_range(self.max_blocks_per_file);
        for _ in 0..num_blocks {
            let num_lines = self.rand_in_range(self.max_lines_per_block);
            let block_name = if self.rand_bool() {
                Some(self.rand_string())
            } else {
                None
            };

            let mut then_change_file: Option<PathBuf> = None;
            let mut then_change_block: Option<String> = None;

            let chosen = self.rand_bool();
            if chosen && self.blocks.len() > 0 {
                // Target an existing file + block.
                let r = self.rand_in_range(blocks_len);
                let (p, b) = self.blocks[r].clone();
                then_change_file = Some(p);
                then_change_block = if self.rand_in_range(100) < 25 {
                    // 25% chance to only use a file target.
                    None
                } else {
                    Some(b)
                };
            } else if chosen && blocks.len() > 0 {
                // 50% to target existing in-file block (assuming we have one!).
                let n = self.rand_in_range(blocks.len());
                then_change_block = Some(blocks[n].clone());
            }

            let block_name_str = block_name.as_deref().unwrap_or("");
            let then_change_file = then_change_file
                .as_ref()
                .map(|p| p.to_str().unwrap())
                .unwrap_or("");
            let then_change_block = then_change_block.as_deref().unwrap_or("");
            let then_change_target = if then_change_file != "" || then_change_block != "" {
                if then_change_block != "" {
                    format!("{}:{}", then_change_file, then_change_block)
                } else {
                    format!("{}", then_change_file)
                }
            } else {
                // This can happen if we're generating the first block ever.
                String::new()
            };

            f.write(format!("LINT.OnChange({})\n", block_name_str).as_bytes())
                .unwrap();
            for _ in 0..num_lines {
                // Write a bunch of empty lines.
                f.write("\n".as_bytes()).unwrap();
            }
            f.write(format!("LINT.ThenChange({})\n", then_change_target,).as_bytes())
                .unwrap();

            if let Some(block_name) = block_name {
                blocks.push(block_name);
            }
        }

        blocks
    }
}
