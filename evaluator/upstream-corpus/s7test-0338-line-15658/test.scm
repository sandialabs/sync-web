;; Imported from upstream s7test.scm line 15658.
;; Original form:
;; (test (let ((lst (list 1))) (set! (car lst) (cdr lst)) (object->string lst)) "(())")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((lst (list 1))) (set! (car lst) (cdr lst)) (object->string lst)))))
       (expected (upstream-safe (lambda () "(())")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15658 actual expected ok?))
