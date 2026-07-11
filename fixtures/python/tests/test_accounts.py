# Requirement: The system shall expose normalize email
def test_normalize_email():
    assert normalize_email(" User@Example.com ") == "user@example.com"


def test_placeholder_is_not_evidence():
    assert True
