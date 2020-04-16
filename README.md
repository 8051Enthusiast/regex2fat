regex2fat
=========

Did you ever want to match a regex, but all you had was a fat32 driver?
Ever wanted to serialize your regex DFAs into one of the most widely supported formats used by over 3 billion devices?
[Are directory loops your thing?](https://xkcd.com/981/)

Worry no more, with `regex2fat` this has become easier than ever before!
With just a little `regex2fat '[YOUR] F{4}VOUR{1,7}E (R[^E]G)*EX HERE.' /dev/whatever`, you will have a fat32 regex DFA of your favourite regex.
For example, to see whether the string `'Y FFFFVOURRE EX HEREM'` would match, just mount it and check if `'/Y/SPACE/F/F/F/F/V/O/U/R/R/E/SPACE/E/X/SPACE/H/E/R/E/M/MATCH'` exists.

To run it, you can [install cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html) and then run `cargo install regex2fat` (or compile it directly from this repo).
If you have the cargo bin directory in your path, you should be able to invoke it like described above.
The file created will be a fat32 image, which can probably be mounted or put on a drive in some way, but most likely shouldn't.

## FAQ
### Q: How does this work?
A: Regular regexes (i.e. no backreferences and similar advanced features) can be turned into a so called DFA (deterministic finite automaton).
This is basically a bunch of arrows going between states, where an arrow is labeled with a letter so that a letter in a state causes the current state to go along the arrow to another state, with a subset of states being accepting.
Yes, I'm bad at explaining, you're better off reading [the wikipedia article on DFAs](https://en.wikipedia.org/wiki/Deterministic_finite_automaton) if you don't know what it is.

Because I'm lazy, I used [BurntSushi/regex-automata](https://github.com/BurntSushi/regex-automata) to get an DFA from a regex.

While Fat32 normally has a tree-like structure, each directory just references blocks anywhere on the file system, so the same block can be referenced from multiple directories.
The directories also have no explicit field for parent directories, so one can leave `..` out.
This allows for graph structures inside a file system, which a DFA basically is.

### Q: Should I use this <del>in production</del> anywhere?
A: No, but I can't stop you.

### Q: Does this actually work?
A: I've tried it on Windows 10 and Linux so far.
It seems to work flawlessly on Windows as far as I've tested.

On Linux, the fat32 code claims an directory is invalid if there are two dentries with the same directory name and the same parent in a loop (or something like that), so some paths are forbidden.

Might be fun to try on some embedded devices.

### Q: NOOOOOOOOOOO!!! YOU CAN'T TURN A DFA INTO A FAT32 FILE SYSTEM!!!! YOU CAN'T JUST HAVE A DIRECTORY WITH MULTIPLE PARENTS!!! YOU ARE BREAKING THE ASSUMPTION OF LACK OF LOOPERINOS NOOOOOOOOO
A: Haha OS-driven regex engine go brrrrr
