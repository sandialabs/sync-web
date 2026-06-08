;; Imported from upstream s7test.scm line 16710.
;; Original form:
;; (test (catch #t (lambda () (make-hash-table 8 eq? (cons integer? (lambda (x) x)))) (lambda (type info) (apply format #f info)))
;;         "make-hash-table: in the third argument, (integer? . #<lambda (x)>), (the key/value type checkers) the second function is anonymous")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (make-hash-table 8 eq? (cons integer? (lambda (x) x)))) (lambda (type info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "make-hash-table: in the third argument, (integer? . #<lambda (x)>), (the key/value type checkers) the second function is anonymous")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16710 actual expected ok?))
