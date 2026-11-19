"""Tests for scripts/check_case_collisions.py."""

import unittest

import check_case_collisions


class Collisions(unittest.TestCase):
    def test_distinct_names_pass(self):
        self.assertEqual(check_case_collisions.collisions(["a/words.ts", "a/Done.tsx"]), [])

    def test_a_file_that_differs_by_case_is_found(self):
        paths = ["a/words.ts", "a/Words.tsx", "a/Words.ts"]
        self.assertEqual(check_case_collisions.collisions(paths), [["a/Words.ts", "a/words.ts"]])

    def test_a_directory_that_differs_by_case_is_found(self):
        paths = ["ui/Src/a.ts", "ui/src/b.ts"]
        self.assertEqual(check_case_collisions.collisions(paths), [["ui/Src", "ui/src"]])

    def test_the_same_path_twice_is_not_a_collision(self):
        self.assertEqual(check_case_collisions.collisions(["a/b", "a/b"]), [])


if __name__ == "__main__":
    unittest.main()
