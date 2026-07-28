(lambda (make-interface-harness)
  ;; Temporary retention prunes old entries from the log chain. The remaining
  ;; branch digests must still derive an exact recent route after the live chain
  ;; crosses a log-layout boundary.
  (with-let (make-interface-harness :journals 2 :users '(alice bob) :window 8)

    (define (directory? value)
      (and (pair? value) (eq? (car value) 'directory)))

    (define (bulk-advance journal count)
      (raw journal
        `(*call* ,(journal 'root-secret)
          (lambda (root)
            (let* ((ledger (sync-eval ((root 'get) '(root object ledger))))
                   (identity-id
                    (cadr
                     (assoc 'id
                            (cadr
                             (assoc 'identity
                                    (cadr
                                     (assoc 'public ((ledger 'config)))))))))
                   (keys
                    (crypto-generate
                     (expression->byte-vector
                      (list 'sync-web/journal-signing-key/v1
                            identity-id
                            (sync-hash
                             (expression->byte-vector
                              ,(journal 'root-secret))))))))
              (let loop ((i 0))
                (if (< i ,count)
                    (begin
                      ((ledger 'set!) '(*state* bootstrap tick)
                       (expression->byte-vector i))
                      ((ledger 'step!)
                       `((unix-time ,i)
                         (public-key ,(car keys))
                         (secret-key ,(cdr keys))))
                      (loop (+ i 1)))))
              ((root 'set!) '(root object ledger) (ledger))
              ((ledger 'size)))))))

    (test-report)
    (test-submit (bulk-advance journal-1 8) :expect 8)
    (test-submit (bulk-advance journal-2 8) :expect 8)
    (test-report)

    (test-submit ((*journal* journal-1 'set!) '(*state* alice local) "origin") :expect #t)
    (test-submit ((*journal* journal-2 'set!) '(*state* alice data private key) "v8") :expect #t)
    (test-submit ((*journal* journal-2 'set!) '(*state* bob data public key) "b8") :expect #t)
    (test-submit
      ((*journal* journal-2 'authorize!)
       '((user (*state* alice))
         (rule ((principal (journal-1 *state* alice))
                (key-index (-20 -1)) (path (data))
                (get #t) (set! #t) (resolve #t)))))
      :expect #t)
    (test-submit ((*journal* journal-1 'step!)) :expect 9)
    (test-submit ((*journal* journal-2 'step!)) :expect 9)
    (test-report)

    (test-submit ((*journal* journal-1 'bridge!) journal-2) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-2 'step!)) :expect 10)
    (test-submit ((*journal* journal-1 'bridge!) journal-2) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-1 'step!)) :expect 10)
    (test-report)
    (test-submit
      ((alice journal-1 journal-2 'resolve :history '(9 -1))
       '(-1 *state*) :pinned? #f :proof? #f)
      :expect directory?)
    (test-report)

    ;; Move from size 10 to the size-16 layout boundary while selected index 9
    ;; remains within the configured retention window. A single raw action keeps
    ;; the regression focused on retained chain structure rather than transport.
    (test-submit (bulk-advance journal-1 6) :expect 16)
    (test-report)

    (test-submit
      ((alice journal-1 journal-2 'resolve :history '(9 -1))
       '(-1 *state*) :pinned? #f :proof? #f)
      :expect directory?)
    (test-report)))
