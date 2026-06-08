;; Imported from upstream s7test.scm line 21841.
;; Original form:
;; (test (format #f "~002c" #\a) "aa")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (format #f "~002c" #\a))))
       (expected (upstream-safe (lambda () "aa")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 21841 actual expected ok?))
