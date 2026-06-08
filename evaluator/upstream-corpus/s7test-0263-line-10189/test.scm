;; Imported from upstream s7test.scm line 10189.
;; Original form:
;; (test (let ((tree1 (list ''a (list ''b ''c))) (tree2 (list ''a (list ''b ''c)))) tree2) '('a ('b 'c)))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((tree1 (list ''a (list ''b ''c))) (tree2 (list ''a (list ''b ''c)))) tree2))))
       (expected (upstream-safe (lambda () '('a ('b 'c)))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10189 actual expected ok?))
