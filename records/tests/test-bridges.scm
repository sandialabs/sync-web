(lambda (assertions-src root-src standard-src chain-src tree-src ledger-src document-src interface-src)

  (define interface-src
    (let recurse ((src interface-src))
      (cond ((or (not (list? src)) (null? src)) src)
            ((equal? (car src) 'sync-remote)
             `(sync-call (append ,(caddr src) (list (list 'authentication ,(cadr src))))
                         #t
                         (sync-hash (expression->byte-vector ,(cadr src)))))
            ((equal? (car src) 'sync-call)
             (append `(sync-call ,(cadr src) #t) (cdddr src)))
            ((list? (car src))
             (cons (recurse (car src)) (recurse (cdr src))))
            (else (cons (car src) (recurse (cdr src)))))))

  (eval assertions-src)

  (define (journal-id name)
    (sync-hash (expression->byte-vector name)))

  (define* (interface-config clear? root-secret interface-secret admins window push-enabled?)
    `((clear? ,clear?)
      (root-secret ,root-secret)
      (interface-secret ,interface-secret)
      (admins ,admins)
      (window ,window)
      (root ,root-src)
      (interface ,interface-secret)
      (name ,interface-secret)
      (push-enabled? ,push-enabled?)))

  (define* (journal-install journal admin-secret interface-secret push-enabled?)
    (sync-call `(,interface-src ,(interface-config #t admin-secret interface-secret '() 4 push-enabled?)
                                ',standard-src ',chain-src ',tree-src ',ledger-src ',document-src)
               #t journal))

  (define (journal-query journal query)
    (sync-call query #t journal))

  (define* (interface-query journal interface query (identity #f))
    (journal-query journal (append query `((authentication (,@(if identity `((identity ,identity)) '()) (credentials ,interface)))))))

  (define (admin-step journal secret)
    (journal-query journal `(*step* ,secret)))

  (define (bridge-local interface policy role remote-name)
    `((interface ,interface)
      (policy ,policy)
      (role ,role)
      (remote-name ,remote-name)))

  (define (error? x)
    (and (list? x) (eq? (car x) 'error)))

  (define (error-tag? tag)
    (lambda (x) (and (error? x) (or (eq? (cadr x) tag)
                                    (and (pair? (cadr x)) (eq? (caadr x) 'quote) (eq? (cadadr x) tag))))))

  (define interface-a "http://journal-a.test/interface")
  (define interface-b "http://journal-b.test/interface")
  (define interface-c "http://journal-c.test/interface")
  (define journal-a (journal-id interface-a))
  (define journal-b (journal-id interface-b))
  (define journal-c (journal-id interface-c))

  (sync-create journal-a)
  (sync-create journal-b)
  (sync-create journal-c)

  (assert (journal-install journal-a "pass-a" interface-a #t) "Installed interface")
  (assert (journal-install journal-b "pass-b" interface-b #f) "Installed interface")
  (assert (journal-install journal-c "pass-c" interface-c #f) "Installed interface")

  ;; Public info exposes default bridge policy for bootstrap without transferring sync payloads.
  (assert (journal-query journal-a '((function info) (arguments ((subscriber journal-b)))))
          (lambda (info)
            (equal? (cadr (assoc 'bridge-policy info)) '((publish push) (subscribe pull)))))

  ;; Default publisher push + subscriber pull negotiates push.
  (let ((query `((function bridge!) (arguments ((name journal-a) (info-local ,(bridge-local interface-a '((publish push) (subscribe pull)) #f 'journal-a)))))))
    (assert (interface-query journal-b interface-b query) #t))

  (assert (interface-query journal-b interface-b '((function config) (arguments ((path (private bridge journal-a policy mode))))))
          'push)

  ;; Publisher creates a signed block. Subscriber receives the normal sync payload by push.
  (assert (interface-query journal-a interface-a
                           '((function set!)
                             (arguments ((path (*state* publisher doc))
                                         (value "from-a")
                                         (meta ((source bridge-test)))
                                         (expression? #t)))))
          #t)
  (assert (admin-step journal-a "pass-a") 1)

  (let ((payload (journal-query journal-a '((function synchronize) (arguments ((index -1)))))))
    (assert (journal-query journal-b `((function synchronize!)
                                      (arguments ((name journal-a)
                                                  (index -1)
                                                  (response ,payload)))))
            (lambda (ack)
              (and (equal? (cadr (assoc 'ok? ack)) #t)
                   (equal? (cadr (assoc 'mode ack)) 'push)))))


  (assert (interface-query journal-b interface-b
                           '((function get)
                             (arguments ((path (*transition* operation))))))
          (lambda (op)
            (and (equal? (cadr (assoc 'function op)) 'synchronize!)
                 (equal? (cadr (assoc 'path op)) '(*bridge* journal-a)))))

  (assert (admin-step journal-b "pass-b") 1)

  (assert (interface-query journal-b interface-b
                           '((function resolve)
                             (arguments ((path (-1 *bridge* journal-a *state* publisher doc))
                                         (expression? #t)
                                         (pinned? #f)
                                         (proof? #f)))))
          "from-a")

  (assert (cadr (assoc 'meta (interface-query journal-b interface-b
                                           '((function resolve)
                                             (arguments ((path (-1 *bridge* journal-a *state* publisher doc))
                                                         (meta? #t)
                                                         (pinned? #f)
                                                         (proof? #f)))))))
          '((source bridge-test)))

  ;; A stale pushed payload is rejected with a non-fatal bridge sync error.
  (let ((payload (journal-query journal-a '((function synchronize) (arguments ((index -1)))))))
    (assert (journal-query journal-b `((function synchronize!)
                                      (arguments ((name journal-a)
                                                  (index -1)
                                                  (response ,payload)))))
            error?))

  ;; Explicit pull-mode negotiation rejects pushed payloads with bridge-mode-error.
  (let* ((info-a (journal-query journal-a '((function info))))
         (pull-info (let loop ((items info-a))
                      (cond ((null? items) '())
                            ((eq? (caar items) 'bridge-policy)
                             (cons '(bridge-policy ((publish pull) (subscribe pull))) (cdr items)))
                            (else (cons (car items) (loop (cdr items))))))))
    (assert (interface-query journal-c interface-c
                             `((function bridge!)
                               (arguments ((name journal-a)
                                           (info-local ,(bridge-local interface-a '((publish push) (subscribe pull)) #f 'journal-a))
                                           (info-remote ,pull-info)))))
            #t)
    (assert (interface-query journal-c interface-c
                             '((function config) (arguments ((path (private bridge journal-a policy mode))))))
            'pull)
    (let ((payload (journal-query journal-a '((function synchronize) (arguments ((index -1)))))))
      (assert (journal-query journal-c `((function synchronize!)
                                        (arguments ((name journal-a)
                                                    (index -1)
                                                    (response ,payload)))))
              error?)))

  ;; Any `none` disables the current bridge config and staged exposure.
  (let ((info-a (journal-query journal-a '((function info)))))
    (assert (interface-query journal-c interface-c
                             `((function bridge!)
                               (arguments ((name journal-a)
                                           (info-local ,(bridge-local interface-a '((publish push) (subscribe none)) #f 'journal-a))
                                           (info-remote ,info-a)))))
            #f)
    (assert (interface-query journal-c interface-c
                             '((function config) (arguments ((path (private bridge journal-a))))))
            '()))

  ;; Publisher-initiated public push: A proposes that C call it `journal-a` and C
  ;; accepts the optimistic first pushed payload without pre-existing bridge config.
  (let ((query `((function bridge!)
                 (arguments ((name journal-c)
                             (info-local ,(bridge-local interface-c '((publish push) (subscribe pull)) 'publisher 'journal-a)))))))
    (assert (interface-query journal-a interface-a query) #t))

  (assert (interface-query journal-a interface-a
                           '((function set!)
                             (arguments ((path (*state* publisher pushed-doc))
                                         (value "optimistic")
                                         (expression? #t)))))
          #t)

  (assert (admin-step journal-a "pass-a") 2)
  (assert (admin-step journal-c "pass-c") 1)

  (assert (interface-query journal-c interface-c
                           '((function resolve)
                             (arguments ((path (-1 *bridge* journal-a *state* publisher pushed-doc))
                                         (expression? #t)
                                         (pinned? #f)
                                         (proof? #f)))))
          "optimistic")

  (append "Success (" (object->string asserted) " checks)"))
