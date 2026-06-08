;; Imported from upstream s7test.scm line 16713.
;; Original form:
;; (test (catch #t (lambda () (make-hash-table 8 eq? (cons (lambda (a b) a) integer?))) (lambda (type info) (apply format #f info)))
;;         "make-hash-table: in the third argument, (#<lambda (a b)> . integer?), (the key/value type checkers) both functions should take one argument")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (make-hash-table 8 eq? (cons (lambda (a b) a) integer?))) (lambda (type info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "make-hash-table: in the third argument, (#<lambda (a b)> . integer?), (the key/value type checkers) both functions should take one argument")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16713 actual expected ok?))
