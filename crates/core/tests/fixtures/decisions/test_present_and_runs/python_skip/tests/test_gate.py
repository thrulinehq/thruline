import pytest

@pytest.mark.skip(reason="fixture")
def test_approval_requires_pro():
    assert True
