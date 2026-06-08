;; Imported from upstream s7test.scm line 10063.
;; Original form:
;; (test (let ((x 2) (lst '(1 2))) (list-set! (let () (set! x 3) lst) 1 23) (list x lst)) '(3 (1 23)))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((x 2) (lst '(1 2))) (list-set! (let () (set! x 3) lst) 1 23) (list x lst)))))
       (expected (upstream-safe (lambda () '(3 (1 23)))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 10063 actual expected ok?))
