from src.duvet.structures import Annotation, AnnotationLevel, AnnotationType


def test_annotation():
    test_anno = Annotation("test_target.md#target", AnnotationType.CITATION, "content", 1, 2,
                          "test_target#target$content", "code.py")
    assert test_anno.target == "test_target.md#target"
    assert test_anno.type == AnnotationType.CITATION
    assert test_anno.content == "content"
    assert test_anno.start_line == 1
    assert test_anno.end_line == 2
    assert test_anno.id == "test_target#target$content"
    assert test_anno.location == "code.py"
