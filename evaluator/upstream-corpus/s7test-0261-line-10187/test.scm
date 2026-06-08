;; Imported from upstream s7test.scm line 10187.
;; Original form:
;; (test (let ((tree1 (list 1 (list 1 2))) (tree2 (list 1 (list 1 2)))) tree2) '(1 (1 2)))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((tree1 (list 1 (list 1 2))) (tree2 (list 1 (list 1 2)))) tree2))))
       (expected (upstream-safe (lambda () '(1 (1 2)))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10187 actual expected ok?))
