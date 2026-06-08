;; Imported from upstream s7test.scm line 21833.
;; Original form:
;; (test (format #f "~C~c~C" #\a #\b #\c) "abc")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~C~c~C" #\a #\b #\c))))
       (expected (upstream-safe (lambda () "abc")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21833 actual expected ok?))
