"""Tests for scripts/check_case_collisions.py."""

import unittest

import check_case_collisions


class Collisions(unittest.TestCase):
    def test_distinct_names_pass(self):
        self.assertEqual(check_case_collisions.collisions(["a/words.ts", "a/Done.tsx"]), [])

    def test_the_same_module_in_two_languages_is_not_a_collision(self):
        self.assertEqual(check_case_collisions.collisions(["a/x.ts", "a/x.test.ts"]), [])

    def test_a_file_that_differs_by_case_is_found(self):
        paths = ["a/words.ts", "a/Words.ts"]
        self.assertEqual(check_case_collisions.collisions(paths), [["a/Words.ts", "a/words.ts"]])

    def test_a_directory_that_differs_by_case_is_found(self):
        paths = ["ui/Src/a.ts", "ui/src/b.ts"]
        self.assertEqual(check_case_collisions.collisions(paths), [["ui/Src", "ui/src"]])

    def test_modules_that_differ_by_case_and_extension_are_found(self):
        paths = ["a/journalQuestion.ts", "a/JournalQuestion.tsx"]
        self.assertEqual(
            check_case_collisions.collisions(paths),
            [["a/JournalQuestion.tsx", "a/journalQuestion.ts"]],
        )

    def test_a_module_and_its_stylesheet_pass(self):
        paths = ["a/Page.tsx", "a/Page.module.css", "a/page.css"]
        self.assertEqual(check_case_collisions.collisions(paths), [])

    def test_the_same_path_twice_is_not_a_collision(self):
        self.assertEqual(check_case_collisions.collisions(["a/b", "a/b"]), [])


if __name__ == "__main__":
    unittest.main()
