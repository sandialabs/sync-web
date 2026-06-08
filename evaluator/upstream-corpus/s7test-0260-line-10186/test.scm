;; Imported from upstream s7test.scm line 10186.
;; Original form:
;; (test (let ((tree1 (list 1 (list 1 2) (list 1 2 3) (list 1 2 3 4)))) tree1) '(1 (1 2) (1 2 3) (1 2 3 4)))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((tree1 (list 1 (list 1 2) (list 1 2 3) (list 1 2 3 4)))) tree1))))
       (expected (upstream-safe (lambda () '(1 (1 2) (1 2 3) (1 2 3 4)))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10186 actual expected ok?))
