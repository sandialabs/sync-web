;; Imported from upstream s7test.scm line 10188.
;; Original form:
;; (test (let ((tree1 (list 1 (list 1 2))) (tree2 (list 1 (list 1 2)))) (eqv? tree1 tree2)) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((tree1 (list 1 (list 1 2))) (tree2 (list 1 (list 1 2)))) (eqv? tree1 tree2)))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10188 actual expected ok?))
