;; Imported from upstream s7test.scm line 16707.
;; Original form:
;; (test (catch #t (lambda () (make-hash-table 8 eq? (cons (lambda (x) x) integer?))) (lambda (type info) (apply format #f info)))
;;         "make-hash-table: in the third argument, (#<lambda (x)> . integer?), (the key/value type checkers) the first function is anonymous")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t (lambda () (make-hash-table 8 eq? (cons (lambda (x) x) integer?))) (lambda (type info) (apply format #f info))))))
       (expected (upstream-safe (lambda () "make-hash-table: in the third argument, (#<lambda (x)> . integer?), (the key/value type checkers) the first function is anonymous")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16707 actual expected ok?))
