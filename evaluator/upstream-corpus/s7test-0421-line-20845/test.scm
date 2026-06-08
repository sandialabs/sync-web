;; Imported from upstream s7test.scm line 20845.
;; Original form:
;; (test (char->integer ((with-input-from-string (string (integer->char 255))(lambda () (read-string 1))) 0)) 255)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (char->integer ((with-input-from-string (string (integer->char 255))(lambda () (read-string 1))) 0)))))
       (expected (upstream-safe (lambda () 255)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 20845 actual expected ok?))
