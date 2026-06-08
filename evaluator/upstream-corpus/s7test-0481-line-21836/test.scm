;; Imported from upstream s7test.scm line 21836.
;; Original form:
;; (test  (format #f "1 2~C 3 4" #\null) "1 2\x00; 3 4")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "1 2~C 3 4" #\null))))
       (expected (upstream-safe (lambda () "1 2\x00; 3 4")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21836 actual expected ok?))
