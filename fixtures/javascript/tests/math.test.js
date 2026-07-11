// Requirement: The system shall expose add numbers
test("adds numbers", () => {
  expect(addNumbers(2, 3)).toBe(5);
});

test("placeholder is not evidence", () => {
  expect(true).toBe(true);
});
