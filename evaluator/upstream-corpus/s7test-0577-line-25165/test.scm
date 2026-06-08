;; Imported from upstream s7test.scm line 25165.
;; Original form:
;; (test (object->string (inlet 'a (bacro* (b :rest c) `(+ ,b ,@c))) :readable) "(inlet :a (bacro* (b :rest c) (list-values '+ b (apply-values c))))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (bacro* (b :rest c) `(+ ,b ,@c))) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (bacro* (b :rest c) (list-values '+ b (apply-values c))))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25165 actual expected ok?))
