;; Imported from upstream s7test.scm line 10185.
;; Original form:
;; (test (let ((tree2 (list "one" (list "one" "two") (list (list "one" "two" "three"))))) tree2) '("one" ("one" "two") (("one" "two" "three"))))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((tree2 (list "one" (list "one" "two") (list (list "one" "two" "three"))))) tree2))))
       (expected (upstream-safe (lambda () '("one" ("one" "two") (("one" "two" "three"))))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10185 actual expected ok?))
