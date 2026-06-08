;; Imported from upstream s7test.scm line 21698.
;; Original form:
;; (test (let ((lst (cons 1 2))) (set-cdr! lst lst) (format #f "~{~A~}" lst)) "1")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((lst (cons 1 2))) (set-cdr! lst lst) (format #f "~{~A~}" lst)))))
       (expected (upstream-safe (lambda () "1")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21698 actual expected ok?))
